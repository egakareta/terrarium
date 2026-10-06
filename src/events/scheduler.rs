use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Weak,
};

use super::EventContext;

type Invocation = Box<dyn FnOnce(&mut EventContext<'_>)>;

pub(super) trait SignalOwner {
    fn close(&self);
}

pub(crate) struct EventScheduler {
    pending: RefCell<VecDeque<Invocation>>,
    dispatching: Cell<bool>,
    signals: RefCell<Vec<Weak<dyn SignalOwner>>>,
    registrations_since_cleanup: Cell<usize>,
    cleanup_threshold: Cell<usize>,
}

impl Default for EventScheduler {
    fn default() -> Self {
        Self {
            pending: RefCell::default(),
            dispatching: Cell::new(false),
            signals: RefCell::default(),
            registrations_since_cleanup: Cell::new(0),
            cleanup_threshold: Cell::new(64),
        }
    }
}

impl EventScheduler {
    pub(crate) fn cleanup_threshold(&self) -> usize {
        self.cleanup_threshold.get()
    }

    pub(crate) fn set_cleanup_threshold(&self, threshold: usize) {
        self.cleanup_threshold.set(threshold.max(1));
    }

    pub(super) fn register(&self, signal: Weak<dyn SignalOwner>) {
        let mut signals = self.signals.borrow_mut();
        let registrations = self.registrations_since_cleanup.get() + 1;
        if registrations >= self.cleanup_threshold.get() {
            signals.retain(|signal| signal.strong_count() != 0);
            self.registrations_since_cleanup.set(0);
        } else {
            self.registrations_since_cleanup.set(registrations);
        }
        signals.push(signal);
    }

    pub(crate) fn enqueue(&self, invocation: Invocation) {
        self.pending.borrow_mut().push_back(invocation);
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty()
    }

    pub(crate) fn dispatch(&self, context: &mut EventContext<'_>, limit: usize) -> usize {
        if self.dispatching.replace(true) {
            return 0;
        }
        let _guard = DispatchGuard(&self.dispatching);
        let mut processed = 0;
        while processed < limit {
            let invocation = self.pending.borrow_mut().pop_front();
            let Some(invocation) = invocation else {
                break;
            };
            invocation(context);
            processed += 1;
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
