use std::rc::Weak;

pub(super) trait ConnectionControl {
    fn disconnect(&self);
    fn is_connected(&self) -> bool;
}

/// An explicitly persistent handle to a signal subscription.
///
/// Dropping this handle does **not** disconnect the callbacks. Use [`Self::disconnect`]
/// for explicit cleanup or [`Self::scoped`] for a subscription that disconnects when
/// its guard is dropped.
/// [`crate::Signal::connect`] returns a scoped guard by default; obtain this
/// handle with [`ScopedConnection::detach`] when the owner should retain it.
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
        ScopedConnection(Some(self))
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
#[must_use = "store this subscription guard; dropping it disconnects the callback"]
pub struct ScopedConnection(Option<Connection>);

impl ScopedConnection {
    /// Disconnects the guarded subscription immediately.
    pub fn disconnect(&self) {
        if let Some(connection) = &self.0 {
            connection.disconnect();
        }
    }

    /// Returns whether the guarded subscription is still connected.
    pub fn is_connected(&self) -> bool {
        self.0.as_ref().is_some_and(Connection::is_connected)
    }

    /// Leaves the subscription attached until explicit disconnection or owner shutdown.
    ///
    /// Use this only when the signal owner's lifetime should own the callback.
    /// Dropping the returned [`Connection`] does not disconnect it.
    pub fn detach(mut self) -> Connection {
        self.0
            .take()
            .expect("subscription guard owns its connection")
    }
}

impl Drop for ScopedConnection {
    fn drop(&mut self) {
        self.disconnect();
    }
}
