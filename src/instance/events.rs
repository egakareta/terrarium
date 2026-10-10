use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

use crate::{
    Attributes, Instance, InstanceId, Signal,
    events::{EventScheduler, SignalEmitter},
};

/// An instance property supported by change notifications.
///
/// Built-in setters emit coalesced invalidations: read the latest value when
/// handling one. Direct edits to public material-slot storage are not observed.
/// Custom metadata uses the separate attribute signals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum InstanceProperty {
    /// The display name changed.
    Name,
    /// The direct parent changed.
    Parent,
    /// The world transform, including position, orientation, or scale, changed.
    Transform,
    /// The part's tint changed.
    Color,
    /// The part's transparency changed.
    Transparency,
    /// The part's anchored state changed.
    Anchored,
    /// The part's collision setting changed.
    CanCollide,
    /// The primitive shape changed.
    Shape,
    /// A material was assigned through a part's material setters.
    Material,
}

impl InstanceProperty {
    fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Parent => "parent",
            Self::Transform => "transform",
            Self::Color => "color",
            Self::Transparency => "transparency",
            Self::Anchored => "anchored",
            Self::CanCollide => "can_collide",
            Self::Shape => "shape",
            Self::Material => "material",
        }
    }
}

/// Owned metadata captured when an instance is added or before it is removed.
///
/// Lifecycle handlers can inspect this data even after the instance is gone.
/// Descendant notifications each carry their own snapshot; `children` contains
/// the direct children's identifiers at the time of the notification.
#[derive(Clone, Debug, PartialEq)]
pub struct InstanceSnapshot {
    /// The instance's stable identifier.
    pub id: InstanceId,
    /// The concrete class name.
    pub class_name: &'static str,
    /// The display name at emission time.
    pub name: String,
    /// The direct parent at emission time, before removal for removal events.
    pub parent: Option<InstanceId>,
    /// Custom metadata at emission time.
    pub attributes: Attributes,
    /// Direct children's identifiers at emission time.
    pub children: Vec<InstanceId>,
}

impl InstanceSnapshot {
    pub(crate) fn capture(instance: &dyn Instance) -> Self {
        Self {
            id: instance.id(),
            class_name: instance.class_name(),
            name: instance.name().to_owned(),
            parent: instance.parent(),
            attributes: instance.attributes().clone(),
            children: instance.children().iter().map(|child| child.id()).collect(),
        }
    }
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
    child_added: OnceCell<SignalEmitter<InstanceSnapshot>>,
    child_removed: OnceCell<SignalEmitter<InstanceSnapshot>>,
    descendant_added: OnceCell<SignalEmitter<InstanceSnapshot>>,
    descendant_removing: OnceCell<SignalEmitter<InstanceSnapshot>>,
    ancestry_changed: OnceCell<SignalEmitter<AncestryChanged>>,
    changed: OnceCell<SignalEmitter<InstanceProperty>>,
    attribute_changed: OnceCell<SignalEmitter<String>>,
    destroying: OnceCell<SignalEmitter<InstanceSnapshot>>,
    #[cfg(feature = "physics")]
    touched: OnceCell<SignalEmitter<InstanceId>>,
    #[cfg(feature = "physics")]
    touch_ended: OnceCell<SignalEmitter<InstanceId>>,
    properties: RefCell<BTreeMap<InstanceProperty, SignalEmitter<()>>>,
    attributes: RefCell<BTreeMap<String, SignalEmitter<()>>>,
    destroying_emitted: Cell<bool>,
}

impl InstanceSignals {
    fn signal<T: 'static>(&self, cell: &OnceCell<SignalEmitter<T>>) -> Signal<T> {
        cell.get_or_init(|| SignalEmitter::new(self.inner.scheduler.borrow().clone()))
            .signal()
    }

    pub(crate) fn child_added(&self) -> Signal<InstanceSnapshot> {
        self.signal(&self.inner.child_added)
    }

    pub(crate) fn child_removed(&self) -> Signal<InstanceSnapshot> {
        self.signal(&self.inner.child_removed)
    }

    pub(crate) fn descendant_added(&self) -> Signal<InstanceSnapshot> {
        self.signal(&self.inner.descendant_added)
    }

    pub(crate) fn descendant_removing(&self) -> Signal<InstanceSnapshot> {
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

    pub(crate) fn destroying(&self) -> Signal<InstanceSnapshot> {
        self.signal(&self.inner.destroying)
    }

    #[cfg(feature = "physics")]
    pub(crate) fn touched(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.touched)
    }

    #[cfg(feature = "physics")]
    pub(crate) fn touch_ended(&self) -> Signal<InstanceId> {
        self.signal(&self.inner.touch_ended)
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
        #[cfg(feature = "physics")]
        attach!(touched, touch_ended);
        for emitter in self.inner.properties.borrow().values() {
            emitter.set_scheduler(scheduler.clone());
        }
        for emitter in self.inner.attributes.borrow().values() {
            emitter.set_scheduler(scheduler.clone());
        }
    }

    pub(crate) fn emit_child_added(&self, child: &dyn Instance) {
        if let Some(emitter) = self.inner.child_added.get()
            && emitter.has_listeners()
        {
            emitter.emit(InstanceSnapshot::capture(child));
        }
    }

    pub(crate) fn emit_child_removed(&self, child: &dyn Instance) {
        if let Some(emitter) = self.inner.child_removed.get()
            && emitter.has_listeners()
        {
            emitter.emit(InstanceSnapshot::capture(child));
        }
    }

    pub(crate) fn emit_descendant_added(&self, descendant: &dyn Instance) {
        if let Some(emitter) = self.inner.descendant_added.get()
            && emitter.has_listeners()
        {
            emitter.emit(InstanceSnapshot::capture(descendant));
        }
    }

    pub(crate) fn emit_descendant_removing(&self, descendant: &dyn Instance) {
        if let Some(emitter) = self.inner.descendant_removing.get()
            && emitter.has_listeners()
        {
            emitter.emit(InstanceSnapshot::capture(descendant));
        }
    }

    pub(crate) fn emit_ancestry_changed(&self, event: AncestryChanged) {
        if let Some(emitter) = self.inner.ancestry_changed.get() {
            emitter.emit(event);
        }
    }

    pub(crate) fn emit_changed(&self, property: InstanceProperty) {
        if let Some(emitter) = self.inner.changed.get() {
            emitter.emit_coalesced(property, property.key());
        }
        if let Some(emitter) = self.inner.properties.borrow().get(&property) {
            emitter.emit_coalesced((), "");
        }
    }

    pub(crate) fn emit_attribute_changed(&self, name: &str) {
        if let Some(emitter) = self.inner.attribute_changed.get() {
            emitter.emit_coalesced(name.to_owned(), name);
        }
        if let Some(emitter) = self.inner.attributes.borrow().get(name) {
            emitter.emit_coalesced((), "");
        }
    }

    pub(crate) fn emit_destroying(&self, instance: &dyn Instance) {
        if !self.inner.destroying_emitted.replace(true)
            && let Some(emitter) = self.inner.destroying.get()
            && emitter.has_listeners()
        {
            emitter.emit(InstanceSnapshot::capture(instance));
        }
    }

    #[cfg(feature = "physics")]
    pub(crate) fn emit_touched(&self, other: InstanceId) {
        if let Some(emitter) = self.inner.touched.get() {
            emitter.emit(other);
        }
    }

    #[cfg(feature = "physics")]
    pub(crate) fn emit_touch_ended(&self, other: InstanceId) {
        if let Some(emitter) = self.inner.touch_ended.get() {
            emitter.emit(other);
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
        #[cfg(feature = "physics")]
        close!(touched, touch_ended);
        for emitter in self.inner.properties.borrow().values() {
            emitter.close();
        }
        for emitter in self.inner.attributes.borrow().values() {
            emitter.close();
        }
    }
}
