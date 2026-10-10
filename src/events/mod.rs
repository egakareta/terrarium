mod connection;
mod scheduler;
mod signal;

pub use connection::{Connection, ScopedConnection};
pub use scheduler::EventQueueFull;
pub(crate) use scheduler::EventScheduler;
pub(crate) use signal::SignalEmitter;
pub use signal::{BindableEvent, Signal, SignalClosed, SignalReceiver, SignalWait};

use crate::Workspace;

/// Scene access supplied while a deferred signal callback is executing.
///
/// Callbacks run without outstanding scene or signal-storage borrows, so they
/// may add, change, or destroy instances through this context. Events produced
/// by those changes are queued rather than recursively dispatched.
pub struct EventContext<'a> {
    pub(crate) workspace: &'a mut Workspace,
}

impl EventContext<'_> {
    /// Returns the workspace executing this callback.
    pub fn workspace(&self) -> &Workspace {
        self.workspace
    }

    /// Returns mutable access to the workspace executing this callback.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        self.workspace
    }
}
