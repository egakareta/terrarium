use std::rc::Weak;

pub(super) trait ConnectionControl {
    fn disconnect(&self);
    fn is_connected(&self) -> bool;
}

/// A handle to a signal subscription.
///
/// Dropping this handle does **not** disconnect the callbacks. Use [`Self::disconnect`]
/// for explicit cleanup or [`Self::scoped`] for a subscription that disconnects when
/// its guard is dropped.
#[derive(Clone)]
pub struct Connection {
    pub(super) control: Option<Weak<dyn ConnectionControl>>,
}

impl Connection {
    /// Disconnects the callback and cancels its queued invocations.
    ///
    /// Calling this more than once is harmless. A callback already executing
    /// finishes normally, but will not be invoked again.
    pub fn disconnect(&self) {
        if let Some(control) = self.control.as_ref().and_then(Weak::upgrade) {
            control.disconnect();
        }
    }

    /// Returns whether this subscription can receive new signal emissions.
    pub fn is_connected(&self) -> bool {
        self.control
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|control| control.is_connected())
    }

    /// Wraps this connection in a guard that disconnects it on drop.
    pub fn scoped(self) -> ScopedConnection {
        ScopedConnection(self)
    }
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Connection")
            .field("connected", &self.is_connected())
            .finish()
    }
}

/// A signal subscription that disconnects when this guard is dropped.
#[derive(Debug)]
pub struct ScopedConnection(Connection);

impl ScopedConnection {
    /// Disconnects the guarded subscription immediately.
    pub fn disconnect(&self) {
        self.0.disconnect();
    }

    /// Returns whether the guarded subscription is still connected.
    pub fn is_connected(&self) -> bool {
        self.0.is_connected()
    }
}

impl Drop for ScopedConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}
