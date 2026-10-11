use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::{Rc, Weak},
};

use super::EventContext;

pub(crate) const EVENT_QUEUE_CAPACITY: usize = 4_096;
type Callback = Box<dyn FnOnce(&mut EventContext<'_>)>;

pub(super) struct Invocation {
    pub(super) cancelled: Rc<Cell<bool>>,
    pub(super) callback: Callback,
}

/// The workspace's callback queue could not accept an emission.
///
/// The queue holds at most 4,096 invocations. No callbacks from a rejected
/// user-fired emission are queued; dispatch existing events before retrying.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the workspace event queue is full (capacity: {capacity})")]
pub struct EventQueueFull {
    /// The maximum number of queued callback invocations.
    pub capacity: usize,
}

pub(super) trait SignalOwner {
    fn close(&self);
}

#[derive(Default)]
pub(crate) struct EventScheduler {
    pending: RefCell<VecDeque<Invocation>>,
    dispatching: Cell<bool>,
    signals: RefCell<Vec<Weak<dyn SignalOwner>>>,
    registrations_since_cleanup: Cell<usize>,
    rejected: Cell<usize>,
}

impl EventScheduler {
    pub(super) fn register(&self, signal: Weak<dyn SignalOwner>) {
        let mut signals = self.signals.borrow_mut();
        let registrations = self.registrations_since_cleanup.get() + 1;
        // Grow the interval with the live registry, avoiding quadratic scans
        // when applications subscribe to a large scene.
        if registrations >= 64.max(signals.len()) {
            signals.retain(|signal| signal.strong_count() != 0);
            self.registrations_since_cleanup.set(0);
        } else {
            self.registrations_since_cleanup.set(registrations);
        }
        signals.push(signal);
    }

    fn prune_cancelled(&self) {
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        let (live, cancelled): (VecDeque<_>, VecDeque<_>) = pending
            .into_iter()
            .partition(|invocation| !invocation.cancelled.get());
        *self.pending.borrow_mut() = live;
        // User closure destructors may enqueue events; release the queue borrow.
        drop(cancelled);
    }

    pub(super) fn enqueue(&self, invocations: Vec<Invocation>) -> Result<(), EventQueueFull> {
        if self.pending.borrow().len() + invocations.len() > EVENT_QUEUE_CAPACITY {
            self.prune_cancelled();
        }
        if self.pending.borrow().len() + invocations.len() > EVENT_QUEUE_CAPACITY {
            self.rejected.set(self.rejected.get().saturating_add(1));
            return Err(EventQueueFull {
                capacity: EVENT_QUEUE_CAPACITY,
            });
        }
        self.pending.borrow_mut().extend(invocations);
        Ok(())
    }

    pub(crate) fn has_pending(&self) -> bool {
        self.pending
            .borrow()
            .iter()
            .any(|invocation| !invocation.cancelled.get())
    }

    pub(crate) fn dispatch_budget(&self) -> usize {
        self.pending
            .borrow()
            .iter()
            .filter(|invocation| !invocation.cancelled.get())
            .count()
            .max(1_024)
    }

    pub(crate) fn dispatch(&self, context: &mut EventContext<'_>, limit: usize) -> usize {
        if self.dispatching.replace(true) {
            return 0;
        }
        let _guard = DispatchGuard(&self.dispatching);
        let rejected = self.rejected.replace(0);
        if rejected != 0 {
            log::warn!(
                "workspace event queue rejected {rejected} emissions; inspect Signal::missed_emissions or BindableEvent::fire errors"
            );
        }
        let mut processed = 0;
        while processed < limit {
            let invocation = self.pending.borrow_mut().pop_front();
            let Some(invocation) = invocation else { break };
            if !invocation.cancelled.get() {
                (invocation.callback)(context);
                processed += 1;
            }
        }
        processed
    }
}

impl Drop for EventScheduler {
    fn drop(&mut self) {
        for signal in self
            .signals
            .get_mut()
            .drain(..)
            .filter_map(|signal| signal.upgrade())
        {
            signal.close();
        }
    }
}

impl std::fmt::Debug for EventScheduler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventScheduler")
            .field("pending", &self.pending.borrow().len())
            .finish()
    }
}

struct DispatchGuard<'a>(&'a Cell<bool>);

impl Drop for DispatchGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
