use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use crate::{
    InstanceId, Signal,
    events::{EventScheduler, SignalEmitter},
};

/// An instance property supported by change notifications.
///
/// Transform, material, and other class-specific properties do not yet emit
/// notifications. Custom metadata uses the separate attribute signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum InstanceProperty {
    /// The display name changed.
    Name,
    /// The direct parent changed.
    Parent,
}

/// The instance and its direct parent when its ancestry changed.
///
/// A descendant receives this notification when an ancestor is attached or
/// removed, even if its own direct parent remains the same. By dispatch time,
/// the instance may already have been destroyed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AncestryChanged {
    /// The instance whose ancestry changed.
    pub instance: InstanceId,
    /// Its direct parent when the notification was emitted.
    pub parent: Option<InstanceId>,
}

/// Shared signal storage for an instance's common notifications.
///
/// This is the backing storage used by [`crate::Instance::instance_signals`].
/// Signals allocate listener storage only when an accessor is first used.
/// Cloning this storage shares signals; scene clones instead create fresh storage.
#[doc(hidden)]
#[derive(Clone, Debug, Default)]
pub struct InstanceSignals {
    inner: Rc<SignalStorage>,
}

#[derive(Debug, Default)]
struct SignalStorage {
    scheduler: RefCell<Weak<EventScheduler>>,
    child_added: OnceCell<SignalEmitter<InstanceId>>,
    child_removed: OnceCell<SignalEmitter<InstanceId>>,
    descendant_added: OnceCell<SignalEmitter<InstanceId>>,
    descendant_removing: OnceCell<SignalEmitter<InstanceId>>,
    ancestry_changed: OnceCell<SignalEmitter<AncestryChanged>>,
    changed: OnceCell<SignalEmitter<InstanceProperty>>,
    attribute_changed: OnceCell<SignalEmitter<String>>,
    destroying: OnceCell<SignalEmitter<()>>,
    properties: RefCell<BTreeMap<InstanceProperty, SignalEmitter<()>>>,
    attributes: RefCell<BTreeMap<String, SignalEmitter<()>>>,
    destroying_emitted: Cell<bool>,
}

impl InstanceSignals {
    fn signal<T: 'static>(&self, cell: &OnceCell<SignalEmitter<T>>) -> Signal<T> {
        cell.get_or_init(|| SignalEmitter::new(self.inner.scheduler.borrow().clone()))
            .signal()
    }

    pub(crate) fn child_added(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.child_added)
    }

    pub(crate) fn child_removed(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.child_removed)
    }

    pub(crate) fn descendant_added(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.descendant_added)
    }

    pub(crate) fn descendant_removing(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.descendant_removing)
    }

    pub(crate) fn ancestry_changed(&self) -> Signal<AncestryChanged> {
        self.signal(&self.inner.ancestry_changed)
    }

    pub(crate) fn changed(&self) -> Signal<InstanceProperty> {
        self.signal(&self.inner.changed)
    }

    pub(crate) fn attribute_changed(&self) -> Signal<String> {
        self.signal(&self.inner.attribute_changed)
    }

    pub(crate) fn destroying(&self) -> Signal<()> {
        self.signal(&self.inner.destroying)
    }

    pub(crate) fn property_changed(&self, property: InstanceProperty) -> Signal<()> {
        self.inner
            .properties
            .borrow_mut()
            .entry(property)
            .or_insert_with(|| SignalEmitter::new(self.inner.scheduler.borrow().clone()))
            .signal()
    }

    pub(crate) fn attribute_changed_for(&self, name: &str) -> Signal<()> {
        self.inner
            .attributes
            .borrow_mut()
            .entry(name.to_owned())
            .or_insert_with(|| SignalEmitter::new(self.inner.scheduler.borrow().clone()))
            .signal()
    }

    pub(crate) fn set_scheduler(&self, scheduler: Weak<EventScheduler>) {
        *self.inner.scheduler.borrow_mut() = scheduler.clone();
        macro_rules! attach {
            ($($field:ident),* $(,)?) => {
                $(if let Some(emitter) = self.inner.$field.get() {
                    emitter.set_scheduler(scheduler.clone());
                })*
            };
        }
        attach!(
            child_added,
            child_removed,
            descendant_added,
            descendant_removing,
            ancestry_changed,
            changed,
            attribute_changed,
            destroying
        );
        for emitter in self.inner.properties.borrow().values() {
            emitter.set_scheduler(scheduler.clone());
        }
        for emitter in self.inner.attributes.borrow().values() {
            emitter.set_scheduler(scheduler.clone());
        }
    }

    pub(crate) fn emit_child_added(&self, child: InstanceId) {
        if let Some(emitter) = self.inner.child_added.get() {
            emitter.emit(child);
        }
    }

    pub(crate) fn emit_child_removed(&self, child: InstanceId) {
        if let Some(emitter) = self.inner.child_removed.get() {
            emitter.emit(child);
        }
    }

    pub(crate) fn emit_descendant_added(&self, descendant: InstanceId) {
        if let Some(emitter) = self.inner.descendant_added.get() {
            emitter.emit(descendant);
        }
    }

    pub(crate) fn emit_descendant_removing(&self, descendant: InstanceId) {
        if let Some(emitter) = self.inner.descendant_removing.get() {
            emitter.emit(descendant);
        }
    }

    pub(crate) fn emit_ancestry_changed(&self, event: AncestryChanged) {
        if let Some(emitter) = self.inner.ancestry_changed.get() {
            emitter.emit(event);
        }
    }

    pub(crate) fn emit_changed(&self, property: InstanceProperty) {
        if let Some(emitter) = self.inner.changed.get() {
            emitter.emit(property);
        }
        if let Some(emitter) = self.inner.properties.borrow().get(&property) {
            emitter.emit(());
        }
    }

    pub(crate) fn emit_attribute_changed(&self, name: &str) {
        if let Some(emitter) = self.inner.attribute_changed.get() {
            emitter.emit(name.to_owned());
        }
        if let Some(emitter) = self.inner.attributes.borrow().get(name) {
            emitter.emit(());
        }
    }

    pub(crate) fn emit_destroying(&self) {
        if !self.inner.destroying_emitted.replace(true)
            && let Some(emitter) = self.inner.destroying.get()
        {
            emitter.emit(());
        }
    }

    pub(crate) fn close(&self) {
        macro_rules! close {
            ($($field:ident),* $(,)?) => {
                $(if let Some(emitter) = self.inner.$field.get() {
                    emitter.close();
                })*
            };
        }
        close!(
            child_added,
            child_removed,
            descendant_added,
            descendant_removing,
            ancestry_changed,
            changed,
            attribute_changed,
            destroying
        );
        for emitter in self.inner.properties.borrow().values() {
            emitter.close();
        }
        for emitter in self.inner.attributes.borrow().values() {
            emitter.close();
        }
    }
}
