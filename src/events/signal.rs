use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};

use super::{
    Connection, EventContext, EventScheduler, connection::ConnectionControl, scheduler::SignalOwner,
};

type Callback<T> = Box<dyn FnMut(&mut EventContext<'_>, &T)>;

struct Listener<T> {
    owner: Weak<SignalState<T>>,
    callback: RefCell<Option<Callback<T>>>,
    on_closed: RefCell<Option<Box<dyn FnOnce()>>>,
    connected: Cell<bool>,
    cancelled: Cell<bool>,
    consumed: Cell<bool>,
    pending: Cell<usize>,
    once: bool,
}

impl<T> Listener<T> {
    fn remove_disconnected(&self) {
        if let Some(owner) = self.owner.upgrade() {
            owner
                .listeners
                .borrow_mut()
                .retain(|listener| listener.connected.get());
        }
    }

    fn close(&self) {
        self.connected.set(false);
        if self.pending.get() == 0 {
            self.callback.borrow_mut().take();
            let on_closed = self.on_closed.borrow_mut().take();
            if let Some(on_closed) = on_closed {
                on_closed();
            }
        }
    }

    fn invoke(&self, context: &mut EventContext<'_>, value: &T) {
        if self.cancelled.get() || (self.once && self.consumed.replace(true)) {
            return;
        }
        if self.once {
            self.connected.set(false);
            self.remove_disconnected();
        }
        let callback = self.callback.borrow_mut().take();
        let mut guard = CallbackGuard {
            listener: self,
            callback,
        };
        if let Some(callback) = guard.callback.as_mut() {
            callback(context, value);
        }
    }
}

impl<T> ConnectionControl for Listener<T> {
    fn disconnect(&self) {
        self.cancelled.set(true);
        self.connected.set(false);
        self.callback.borrow_mut().take();
        let on_closed = self.on_closed.borrow_mut().take();
        self.remove_disconnected();
        if let Some(on_closed) = on_closed {
            on_closed();
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.get()
    }
}

struct CallbackGuard<'a, T> {
    listener: &'a Listener<T>,
    callback: Option<Callback<T>>,
}

impl<T> Drop for CallbackGuard<'_, T> {
    fn drop(&mut self) {
        if !self.listener.cancelled.get() && !self.listener.once {
            *self.listener.callback.borrow_mut() = self.callback.take();
        }
    }
}

struct PendingInvocation<T>(Rc<Listener<T>>);

impl<T> Drop for PendingInvocation<T> {
    fn drop(&mut self) {
        self.0.pending.set(self.0.pending.get() - 1);
        if !self.0.connected.get() {
            self.0.close();
        }
    }
}

struct SignalState<T> {
    listeners: RefCell<Vec<Rc<Listener<T>>>>,
    scheduler: RefCell<Weak<EventScheduler>>,
    closed: Cell<bool>,
}

impl<T> SignalOwner for SignalState<T> {
    fn close(&self) {
        if self.closed.replace(true) {
            return;
        }
        let listeners = std::mem::take(&mut *self.listeners.borrow_mut());
        for listener in listeners {
            listener.close();
        }
    }
}

/// A typed, read-only handle to an object's deferred signal.
///
/// Cloned handles refer to the same signal. Connecting does not replay earlier
/// emissions. Callbacks run in registration order at workspace event dispatch
/// boundaries, and may access the scene through [`EventContext`].
///
/// Signal handles do not keep their owner alive. Instance signals become closed
/// when their instance is destroyed. Signals on detached instances can be
/// subscribed to, but do not emit until the instance belongs to a workspace.
pub struct Signal<T: 'static> {
    state: Weak<SignalState<T>>,
}

impl<T: 'static> Signal<T> {
    /// Registers a callback for future emissions.
    ///
    /// Dropping the returned connection does not disconnect it. Connecting to a
    /// closed signal returns an already-disconnected handle.
    pub fn connect(&self, callback: impl FnMut(&mut EventContext<'_>, &T) + 'static) -> Connection {
        self.subscribe(Box::new(callback), false, None)
    }

    /// Registers a callback for the next emission only.
    ///
    /// The connection is disconnected before invoking the callback, even when
    /// multiple emissions are already queued.
    pub fn once(&self, callback: impl FnOnce(&mut EventContext<'_>, &T) + 'static) -> Connection {
        let mut callback = Some(callback);
        self.subscribe(
            Box::new(move |context, value| {
                if let Some(callback) = callback.take() {
                    callback(context, value);
                }
            }),
            true,
            None,
        )
    }

    /// Waits asynchronously for the next emission, without blocking a thread.
    ///
    /// Subscription starts immediately when this method is called. Dropping the
    /// future cancels its subscription. Closing the signal resolves the future
    /// with [`SignalClosed`], unless an earlier emission is awaiting dispatch.
    pub fn wait(&self) -> SignalWait<T>
    where
        T: Clone,
    {
        let state = Rc::new(WaitState {
            result: RefCell::new(None),
            waker: RefCell::new(None),
            completed: Cell::new(false),
        });
        let completed = state.clone();
        let closed = state.clone();
        let connection = self.subscribe(
            Box::new(move |_, value| completed.complete(Ok(value.clone()))),
            true,
            Some(Box::new(move || closed.complete(Err(SignalClosed)))),
        );
        if !connection.is_connected() {
            state.complete(Err(SignalClosed));
        }
        SignalWait { state, connection }
    }

    /// Returns whether the signal's owner has closed or dropped it.
    pub fn is_closed(&self) -> bool {
        self.state.upgrade().is_none_or(|state| state.closed.get())
    }

    pub(crate) fn closed() -> Self {
        Self { state: Weak::new() }
    }

    fn subscribe(
        &self,
        callback: Callback<T>,
        once: bool,
        on_closed: Option<Box<dyn FnOnce()>>,
    ) -> Connection {
        let Some(state) = self.state.upgrade().filter(|state| !state.closed.get()) else {
            return Connection { control: None };
        };
        let listener = Rc::new(Listener {
            owner: Rc::downgrade(&state),
            callback: RefCell::new(Some(callback)),
            on_closed: RefCell::new(on_closed),
            connected: Cell::new(true),
            cancelled: Cell::new(false),
            consumed: Cell::new(false),
            pending: Cell::new(0),
            once,
        });
        let control: Rc<dyn ConnectionControl> = listener.clone();
        let mut listeners = state.listeners.borrow_mut();
        listeners.retain(|listener| listener.connected.get());
        listeners.push(listener);
        Connection {
            control: Some(Rc::downgrade(&control)),
        }
    }
}

impl<T: 'static> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T: 'static> std::fmt::Debug for Signal<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Signal")
            .field("closed", &self.is_closed())
            .finish()
    }
}

pub(crate) struct SignalEmitter<T: 'static> {
    state: Rc<SignalState<T>>,
}

impl<T: 'static> SignalEmitter<T> {
    pub(crate) fn new(scheduler: Weak<EventScheduler>) -> Self {
        let emitter = Self {
            state: Rc::new(SignalState {
                listeners: RefCell::new(Vec::new()),
                scheduler: RefCell::new(Weak::new()),
                closed: Cell::new(false),
            }),
        };
        emitter.set_scheduler(scheduler);
        emitter
    }

    pub(crate) fn signal(&self) -> Signal<T> {
        Signal {
            state: Rc::downgrade(&self.state),
        }
    }

    pub(crate) fn set_scheduler(&self, scheduler: Weak<EventScheduler>) {
        if self.state.scheduler.borrow().ptr_eq(&scheduler) {
            return;
        }
        if let Some(scheduler) = scheduler.upgrade() {
            let owner: Rc<dyn SignalOwner> = self.state.clone();
            scheduler.register(Rc::downgrade(&owner));
        }
        *self.state.scheduler.borrow_mut() = scheduler;
    }

    pub(crate) fn emit(&self, value: T) {
        if self.state.closed.get() {
            return;
        }
        let Some(scheduler) = self.state.scheduler.borrow().upgrade() else {
            return;
        };
        let mut listeners = self.state.listeners.borrow_mut();
        listeners.retain(|listener| listener.connected.get());
        if listeners.is_empty() {
            return;
        }
        let value = Rc::new(value);
        for listener in listeners.iter() {
            listener.pending.set(listener.pending.get() + 1);
            let pending = PendingInvocation(listener.clone());
            let value = value.clone();
            scheduler.enqueue(Box::new(move |context| pending.0.invoke(context, &value)));
        }
    }

    pub(crate) fn close(&self) {
        self.state.close();
    }
}

impl<T: 'static> Drop for SignalEmitter<T> {
    fn drop(&mut self) {
        self.close();
    }
}

impl<T: 'static> std::fmt::Debug for SignalEmitter<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.signal().fmt(formatter)
    }
}

/// Indicates that a signal closed before a waiting future received an emission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the signal was closed")]
pub struct SignalClosed;

struct WaitState<T> {
    result: RefCell<Option<Result<T, SignalClosed>>>,
    waker: RefCell<Option<Waker>>,
    completed: Cell<bool>,
}

impl<T> WaitState<T> {
    fn complete(&self, result: Result<T, SignalClosed>) {
        if self.completed.replace(true) {
            return;
        }
        let mut current = self.result.borrow_mut();
        *current = Some(result);
        drop(current);
        let waker = self.waker.borrow_mut().take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// A cancellable future returned by [`Signal::wait`].
#[must_use = "signal waits must be polled or awaited"]
pub struct SignalWait<T> {
    state: Rc<WaitState<T>>,
    connection: Connection,
}

impl<T> Future for SignalWait<T> {
    type Output = Result<T, SignalClosed>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(result) = self.state.result.borrow_mut().take() {
            Poll::Ready(result)
        } else {
            *self.state.waker.borrow_mut() = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> Drop for SignalWait<T> {
    fn drop(&mut self) {
        self.connection.disconnect();
    }
}

/// A user-created event associated with a workspace's deferred scheduler.
///
/// Create one with [`crate::Workspace::bindable_event`]. Unlike instance signal
/// handles, its owner may explicitly [`fire`](Self::fire) it. Dropping the event
/// closes its signal; queued callbacks may still execute at the next dispatch.
pub struct BindableEvent<T: 'static> {
    emitter: SignalEmitter<T>,
}

impl<T: 'static> BindableEvent<T> {
    pub(crate) fn new(scheduler: Weak<EventScheduler>) -> Self {
        Self {
            emitter: SignalEmitter::new(scheduler),
        }
    }

    /// Returns the read-only signal used to subscribe to this event.
    pub fn on_event(&self) -> Signal<T> {
        self.emitter.signal()
    }

    /// Queues this payload for the callbacks subscribed at the time of firing.
    pub fn fire(&self, value: T) {
        self.emitter.emit(value);
    }
}
