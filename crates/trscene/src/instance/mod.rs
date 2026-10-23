//! Scene instance types and workspace management.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    fmt::Debug,
    hash::{BuildHasherDefault, Hasher},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::{Mutex, OnceLock},
};

use serde::{Serialize, de::DeserializeOwned};
use slotmap::SlotMap;

pub mod composition;
pub use composition::*;

mod camera;
mod light;
#[cfg(feature = "meshpart")]
mod meshpart;
mod outline;
mod part;
mod pv;
mod rig;
#[cfg(feature = "sound")]
mod sound;
mod workspace;
pub use camera::*;
pub use light::*;
#[cfg(feature = "meshpart")]
pub use meshpart::*;
pub use outline::*;
pub use part::*;
pub use pv::*;
pub(crate) use pv::{InstanceTransformUpdate, update_instance_transform};
pub use rig::*;
#[cfg(feature = "sound")]
pub use sound::*;
pub use workspace::*;

#[derive(Default)]
struct InstanceIdHasher(u64);

impl Hasher for InstanceIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut hash = 0xcbf2_9ce4_8422_2325;
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = hash;
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

type InstanceMap<T> = HashMap<InstanceId, T, BuildHasherDefault<InstanceIdHasher>>;

slotmap::new_key_type! {
    /// Stable identifier for an [`Instance`].
    pub struct InstanceId;
}

static INSTANCE_IDS: OnceLock<Mutex<SlotMap<InstanceId, ()>>> = OnceLock::new();

/// Custom metadata stored on an [`Instance`], keyed by attribute name.
///
/// Kept sorted by key so iteration and debug output are deterministic.
pub type Attributes = BTreeMap<String, serde_json::Value>;

fn instance_ids() -> &'static Mutex<SlotMap<InstanceId, ()>> {
    INSTANCE_IDS.get_or_init(|| Mutex::new(SlotMap::with_key()))
}

/// Internal index used to resolve instances by their stable identifier.
#[doc(hidden)]
#[derive(Debug, Default)]
pub(crate) struct InstanceLookup {
    instances: RefCell<InstanceMap<NonNull<dyn Instance>>>,
    root: Cell<Option<(InstanceId, NonNull<InstanceData>)>>,
    generation: Cell<u64>,
}

impl InstanceLookup {
    /// Registers an instance pointer in this lookup index.
    #[doc(hidden)]
    pub(crate) fn register(&self, instance: &mut dyn Instance) {
        let inserted = self
            .instances
            .borrow_mut()
            .insert(instance.id(), NonNull::from(instance))
            .is_none();
        if inserted {
            self.generation.set(self.generation.get().wrapping_add(1));
        }
    }

    /// Removes an instance from the lookup index.
    #[doc(hidden)]
    pub(crate) fn unregister(&self, id: InstanceId) {
        self.instances.borrow_mut().remove(&id);
        self.generation.set(self.generation.get().wrapping_add(1));
    }

    /// Returns a counter bumped on every structural add/remove in the workspace.
    ///
    /// Property mutations (pivot, color, etc.) do not bump this, so a cached
    /// scan for zero outlines/lights can be safely skipped while the
    /// generation is unchanged: no new instances could have appeared.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.get()
    }

    pub(crate) fn get(&self, id: InstanceId) -> Option<NonNull<dyn Instance>> {
        self.instances.borrow().get(&id).copied()
    }

    fn descendant(&self, root: InstanceId, id: InstanceId) -> Option<NonNull<dyn Instance>> {
        if id == root {
            return None;
        }
        let instance = self.get(id)?;
        // Indexed instances remain boxed while parented; shared indices must stay subtree-local.
        let mut parent = unsafe { instance.as_ref() }.parent();
        while let Some(id) = parent {
            if id == root {
                return Some(instance);
            }
            parent = unsafe { self.get(id)?.as_ref() }.parent();
        }
        None
    }

    pub(crate) fn set_root(&self, root: &mut InstanceData) {
        self.root.set(Some((root.id(), NonNull::from(root))));
    }

    fn remove_child(&self, parent_id: InstanceId, child_id: InstanceId) -> bool {
        if let Some(mut parent) = self.get(parent_id) {
            // Boxed instances have stable addresses while they are owned by the workspace.
            return unsafe { parent.as_mut().remove_child(child_id) };
        }

        let Some((root_id, mut root)) = self.root.get() else {
            return false;
        };
        if root_id != parent_id {
            return false;
        }

        // The workspace root is boxed so this pointer remains valid when Workspace moves.
        unsafe { root.as_mut().remove_child(child_id) }
    }
}

/// Emits inspector implementations when the runtime enables devtools.
#[doc(hidden)]
#[cfg(feature = "devtools")]
#[macro_export]
macro_rules! __instance_inspection_items {
    ($($items:tt)*) => { $($items)* };
}

/// Omits inspector implementations when the runtime disables devtools.
#[doc(hidden)]
#[cfg(not(feature = "devtools"))]
#[macro_export]
macro_rules! __instance_inspection_items {
    ($($items:tt)*) => {};
}

/// Selects spatial support at each concrete expansion site.
#[doc(hidden)]
pub struct __SpatialProbe<T>(pub std::marker::PhantomData<T>);

/// Autoref specialization for generated spatial capability access.
#[doc(hidden)]
pub trait __DetectSpatial<T> {
    /// Returns shared spatial access when implemented.
    fn get(self, instance: &T) -> Option<&dyn Spatial>;
    /// Returns mutable spatial access when implemented.
    fn get_mut(self, instance: &mut T) -> Option<&mut dyn Spatial>;
}
impl<T> __DetectSpatial<T> for &__SpatialProbe<T> {
    fn get(self, _: &T) -> Option<&dyn Spatial> {
        None
    }
    fn get_mut(self, _: &mut T) -> Option<&mut dyn Spatial> {
        None
    }
}
impl<T: Spatial> __DetectSpatial<T> for &&__SpatialProbe<T> {
    fn get(self, instance: &T) -> Option<&dyn Spatial> {
        Some(instance)
    }
    fn get_mut(self, instance: &mut T) -> Option<&mut dyn Spatial> {
        Some(instance)
    }
}

/// Implements the feature-dependent portion of [`crate::instance!`].
#[doc(hidden)]
#[cfg(feature = "devtools")]
#[macro_export]
macro_rules! __impl_instance_inspection {
    ($type:ty) => {
        fn properties(&self) -> Vec<$crate::devtools::Property> {
            <$type as $crate::devtools::Inspect>::properties(self)
        }

        fn validate_property(
            &self,
            path: &str,
            value: &$crate::devtools::PropertyValue,
        ) -> Result<(), $crate::devtools::PropertyError> {
            <$type as $crate::devtools::Inspect>::validate_property(self, path, value)
        }

        fn set_property(
            &mut self,
            path: &str,
            value: $crate::devtools::PropertyValue,
        ) -> Result<(), $crate::devtools::PropertyError> {
            <$type as $crate::devtools::Inspect>::set_property(self, path, value)
        }
    };
}

/// Implements the feature-dependent portion of [`crate::instance!`].
#[doc(hidden)]
#[cfg(not(feature = "devtools"))]
#[macro_export]
macro_rules! __impl_instance_inspection {
    ($type:ty) => {};
}

impl InstanceId {
    fn new() -> Self {
        instance_ids()
            .lock()
            .expect("instance ID registry poisoned")
            .insert(())
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
#[doc(hidden)]
pub struct InstanceData {
    name: String,
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    id: InstanceId,
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    parent: Option<InstanceId>,
    #[cfg_attr(feature = "devtools", inspect(skip))]
    children: Vec<Box<dyn Instance>>,
    #[cfg_attr(feature = "devtools", inspect(skip))]
    sibling_index: usize,
    #[cfg_attr(feature = "devtools", inspect(skip))]
    lookup: Weak<InstanceLookup>,
    #[cfg_attr(feature = "devtools", inspect(skip))]
    root_lookup: Option<Rc<InstanceLookup>>,
    #[cfg_attr(feature = "devtools", inspect(skip))]
    persistent: bool,
    attributes: Attributes,
}

impl Default for InstanceData {
    fn default() -> Self {
        Self::new("Instance")
    }
}

impl InstanceData {
    #[doc(hidden)]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: InstanceId::new(),
            parent: None,
            children: Vec::new(),
            sibling_index: usize::MAX,
            lookup: Weak::new(),
            root_lookup: None,
            persistent: false,
            attributes: Attributes::new(),
        }
    }

    /// Creates stable identity storage with an owned tree index.
    #[doc(hidden)]
    pub fn new_root(name: impl Into<String>) -> Box<Self> {
        let lookup = Rc::new(InstanceLookup::default());
        let mut instance = Box::new(Self::new(name));
        lookup.set_root(&mut instance);
        instance.set_lookup(Some(&lookup));
        instance.root_lookup = Some(lookup);
        instance
    }

    /// Adds a child that cannot be destroyed independently of its owner.
    #[doc(hidden)]
    pub fn add_persistent_child(&mut self, mut child: Box<dyn Instance>) -> InstanceId {
        instance_data_mut(child.as_mut()).persistent = true;
        self.add_child(child)
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub(crate) fn id(&self) -> InstanceId {
        self.id
    }

    pub(crate) fn parent(&self) -> Option<InstanceId> {
        self.parent
    }

    pub(crate) fn set_parent(&mut self, parent: Option<InstanceId>) {
        self.parent = parent;
    }

    pub(crate) fn sibling_index(&self) -> usize {
        self.sibling_index
    }

    pub(crate) fn set_sibling_index(&mut self, index: usize) {
        self.sibling_index = index;
    }

    pub(crate) fn lookup(&self) -> Option<Rc<InstanceLookup>> {
        self.lookup.upgrade()
    }

    pub(crate) fn set_lookup(&mut self, lookup: Option<&Rc<InstanceLookup>>) {
        self.lookup = lookup.map_or_else(Weak::new, Rc::downgrade);
    }

    pub(crate) fn children(&self) -> &[Box<dyn Instance>] {
        &self.children
    }

    /// Iterates over mutable access to the owned children.
    #[doc(hidden)]
    pub fn children_mut(&mut self) -> ChildrenMut<'_> {
        ChildrenMut::new(self.children.iter_mut())
    }

    pub(crate) fn set_attribute(&mut self, name: String, value: serde_json::Value) {
        self.attributes.insert(name, value);
    }

    pub(crate) fn get_attribute(&self, name: &str) -> Option<&serde_json::Value> {
        self.attributes.get(name)
    }

    pub(crate) fn attributes(&self) -> &Attributes {
        &self.attributes
    }

    pub(crate) fn remove_attribute(&mut self, name: &str) -> Option<serde_json::Value> {
        self.attributes.remove(name)
    }

    pub(crate) fn clear_attributes(&mut self) {
        self.attributes.clear();
    }

    pub(crate) fn add_child(&mut self, mut child: Box<dyn Instance>) -> InstanceId {
        let data = instance_data_mut(child.as_mut());
        data.set_parent(Some(self.id));
        data.set_sibling_index(self.children.len());
        let child_id = child.id();
        self.children.push(child);
        if let Some(lookup) = self.lookup() {
            set_instance_lookup(
                self.children
                    .last_mut()
                    .expect("just pushed child")
                    .as_mut(),
                Some(lookup),
            );
        }
        child_id
    }

    pub(crate) fn remove_child(&mut self, id: InstanceId) -> bool {
        let indexed_position = self.lookup().and_then(|lookup| {
            let instance = lookup.get(id)?;
            // Registered instances remain boxed at stable addresses while parented.
            Some(unsafe { instance.as_ref().__instance_data().sibling_index() })
        });
        let Some(index) = indexed_position
            .filter(|&index| {
                self.children
                    .get(index)
                    .is_some_and(|child| child.id() == id)
            })
            .or_else(|| self.children.iter().position(|child| child.id() == id))
        else {
            return false;
        };
        if self.children[index].__instance_data().persistent {
            return false;
        }
        let mut child = self.children.swap_remove(index);
        if let Some(moved_child) = self.children.get_mut(index) {
            instance_data_mut(moved_child.as_mut()).set_sibling_index(index);
        }
        let data = instance_data_mut(child.as_mut());
        data.set_parent(None);
        data.set_sibling_index(usize::MAX);
        set_instance_lookup(child.as_mut(), None);
        true
    }
}

impl Drop for InstanceData {
    fn drop(&mut self) {
        instance_ids()
            .lock()
            .expect("instance ID registry poisoned")
            .remove(self.id);
    }
}

impl Clone for InstanceData {
    fn clone(&self) -> Self {
        let mut cloned = Self::new(self.name.clone());
        cloned.attributes = self.attributes.clone();
        for child in self.children() {
            cloned.add_child(child.clone());
        }
        cloned
    }
}

fn instance_data_mut<T: __InstanceDataAccess + ?Sized>(instance: &mut T) -> &mut InstanceData {
    // SAFETY: Runtime callers update fields in place without moving registered identity storage.
    unsafe { instance.__instance_data_mut() }
}

fn set_instance_lookup(instance: &mut dyn Instance, lookup: Option<Rc<InstanceLookup>>) {
    if let Some(previous) = instance.__instance_data().lookup() {
        previous.unregister(instance.id());
    }
    if let Some(lookup) = &lookup {
        lookup.register(instance);
    }
    instance_data_mut(instance).set_lookup(lookup.as_ref());
    for child in instance.children_mut() {
        set_instance_lookup(child, lookup.clone());
    }
}

/// A node in the object hierarchy.
///
/// Implementors expose their concrete type through [`Any`], which allows a
/// heterogeneous collection such as [`Workspace`] to recover typed instances
/// without requiring callers to maintain parallel type-specific collections.
pub trait Instance: Any + Debug + InstanceClone + __InstanceDataAccess + InstanceReceiver {
    /// Returns the generated composition descriptor, when available.
    fn instance_class(&self) -> Option<&'static InstanceClass> {
        None
    }

    /// Looks up a typed API adapter for this concrete receiver.
    #[doc(hidden)]
    fn query(&self, _target: std::any::TypeId) -> Option<&'static dyn Any> {
        None
    }

    /// Returns this instance's spatial capability, when it has a world-space pivot.
    fn spatial(&self) -> Option<&dyn Spatial> {
        None
    }

    /// Returns mutable access to this instance's spatial capability, when present.
    fn spatial_mut(&mut self) -> Option<&mut dyn Spatial> {
        None
    }

    /// Returns fields exposed to the devtools inspector.
    ///
    /// [`crate::instance!`] exposes annotated properties and inherited APIs.
    #[cfg(feature = "devtools")]
    fn properties(&self) -> Vec<crate::devtools::Property> {
        vec![crate::devtools::Property::new(
            "name",
            crate::devtools::PropertyValue::Text(self.name().to_owned()),
        )]
    }

    /// Validates an edit without changing this instance.
    #[cfg(feature = "devtools")]
    fn validate_property(
        &self,
        path: &str,
        value: &crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        self.clone_box().set_property(path, value.clone())
    }

    /// Applies an edit to a field exposed by [`Self::properties`].
    #[cfg(feature = "devtools")]
    fn set_property(
        &mut self,
        path: &str,
        value: crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        if path != "name" {
            return Err(crate::devtools::PropertyError::NotFound);
        }
        let crate::devtools::PropertyValue::Text(name) = value else {
            return Err(crate::devtools::PropertyError::InvalidValue);
        };
        self.set_name(name);
        Ok(())
    }

    /// The concrete class name of this instance.
    fn class_name(&self) -> &'static str;

    crate::__instance_property! {
        self;
        /// The display name, which defaults to the concrete class name.
        getter name() -> &str { self.__instance_data().name() }
        setter set_name(name: String) { instance_data_mut(self).set_name(name); }
        builder with_name(name: impl Into<String>) { self.set_name(name.into()); }
    }

    /// Returns this instance's stable identifier.
    fn id(&self) -> InstanceId {
        self.__instance_data().id()
    }

    /// Stores a custom metadata value under `name`, overwriting any previous
    /// value.
    ///
    /// This is arbitrary per-instance data that the engine itself ignores. Any
    /// JSON value works, so nested objects and arrays are supported. Prefer
    /// [`InstanceAttributes::with_typed_attribute`] to store a serializable
    /// Rust value without building the JSON by hand.
    ///
    /// ```
    /// use trscene::{Instance, Part};
    ///
    /// let part = Part::new()
    ///     .with_name("crate")
    ///     .with_attribute("health", serde_json::json!(100));
    /// assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(100)));
    /// ```
    fn with_attribute(mut self, name: impl Into<String>, value: serde_json::Value) -> Self
    where
        Self: Sized,
    {
        instance_data_mut(&mut self).set_attribute(name.into(), value);
        self
    }

    /// Returns the custom metadata stored under `name`, if any.
    fn get_attribute(&self, name: &str) -> Option<&serde_json::Value> {
        self.__instance_data().get_attribute(name)
    }

    /// Returns all custom metadata in sorted key order.
    fn attributes(&self) -> &Attributes {
        self.__instance_data().attributes()
    }

    /// Reports whether custom metadata is stored under `name`.
    fn has_attribute(&self, name: &str) -> bool {
        self.get_attribute(name).is_some()
    }

    /// Removes the custom metadata stored under `name`, returning it when present.
    fn remove_attribute(&mut self, name: &str) -> Option<serde_json::Value> {
        instance_data_mut(self).remove_attribute(name)
    }

    /// Removes all custom metadata from this instance.
    fn clear_attributes(&mut self) {
        instance_data_mut(self).clear_attributes();
    }

    /// Returns the parent identifier, or `None` for an isolated instance.
    fn parent(&self) -> Option<InstanceId> {
        self.__instance_data().parent()
    }

    /// Returns this instance's children in insertion order.
    fn children(&self) -> &[Box<dyn Instance>] {
        self.__instance_data().children()
    }

    /// Iterates over mutable access to this instance's children.
    fn children_mut(&mut self) -> ChildrenMut<'_> {
        instance_data_mut(self).children_mut()
    }

    /// Removes a direct child by its stable identifier.
    #[doc(hidden)]
    fn remove_child(&mut self, id: InstanceId) -> bool {
        instance_data_mut(self).remove_child(id)
    }

    /// Returns whether this instance can be destroyed independently of its owner.
    fn is_destroyable(&self) -> bool {
        self.parent().is_some()
            && !self.__instance_data().persistent
            && self.__instance_data().lookup().is_some()
    }

    /// Removes this instance from its parent and destroys its descendants.
    ///
    /// Returns `true` when the instance was removed from its indexed tree.
    /// Roots and persistent children are retained. An isolated instance is
    /// destroyed by dropping its owned value instead.
    ///
    /// This must be the last operation performed through this instance
    /// reference when it returns `true`.
    fn destroy(&mut self) -> bool {
        let Some(parent_id) = self.parent() else {
            return false;
        };
        let id = self.id();
        let Some(lookup) = self.__instance_data().lookup() else {
            return false;
        };

        lookup.remove_child(parent_id, id)
    }

    /// Adds an owned child to this instance.
    fn add_child_box(&mut self, child: Box<dyn Instance>) -> InstanceId {
        instance_data_mut(self).add_child(child)
    }

    /// Adds an owned child to this instance and returns a mutable reference to it.
    ///
    /// The reference borrows the parent, so copy out the [`InstanceId`] with
    /// [`Instance::id`] if the handle must outlive the borrow.
    fn add_child_box_ref(&mut self, child: Box<dyn Instance>) -> &mut dyn Instance {
        self.add_child_box(child);
        self.children_mut().next_back().expect("just pushed child")
    }

    /// Adds an owned child and returns the child's stable identifier.
    fn add_child<T>(&mut self, child: T) -> InstanceId
    where
        Self: Sized,
        T: Instance,
    {
        self.add_child_box(Box::new(child))
    }

    /// Configures an isolated child, then adds it and returns its identifier.
    fn add_child_with<T, F>(&mut self, mut child: T, configure: F) -> InstanceId
    where
        Self: Sized,
        T: Instance,
        F: FnOnce(&mut T),
    {
        configure(&mut child);
        self.add_child(child)
    }

    /// Adds an owned child to this instance and returns a mutable reference to it.
    ///
    /// The concrete type is known statically, so no downcasting is needed.
    /// The reference borrows the parent, so copy out the [`InstanceId`] with
    /// [`Instance::id`] if the handle must outlive the borrow.
    fn add_child_ref<T>(&mut self, child: T) -> &mut T
    where
        Self: Sized,
        T: Instance,
    {
        self.add_child_box_ref(Box::new(child))
            .downcast_mut::<T>()
            .expect("just pushed child of type T")
    }

    /// Returns this value as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn ::std::any::Any {
        self.as_instance()
    }

    /// Returns this value as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
        self.as_instance_mut()
    }

    /// Parents an isolated instance to any instance.
    ///
    /// The instance is moved into the parent, so it can continue to be
    /// accessed through the returned [`InstanceId`] and tree accessors.
    fn set_parent(self, parent: &mut dyn Instance) -> InstanceId
    where
        Self: Sized,
    {
        parent.add_child_box(Box::new(self))
    }

    /// Iterates over all descendants in depth-first insertion order.
    fn descendants(&self) -> Descendants<'_> {
        Descendants::new(self.children())
    }

    /// Finds the first descendant of type `T` with `name` in depth-first order.
    fn find_first_child<T: Instance>(&mut self, name: &str) -> Option<(InstanceId, &mut T)>
    where
        Self: Sized,
    {
        find_child_mut(self, name)
    }
}

fn resolve_instance(instance: &dyn Instance, id: InstanceId) -> Option<&dyn Instance> {
    if instance.id() == id {
        return Some(instance);
    }
    if let Some(lookup) = instance.__instance_data().lookup() {
        let instance = lookup.descendant(instance.id(), id)?;
        // The subtree check confines the pointer to descendants owned by this borrow.
        return Some(unsafe { instance.as_ref() });
    }
    instance
        .children()
        .iter()
        .find_map(|child| resolve_instance(child.as_ref(), id))
}

fn resolve_instance_mut(instance: &mut dyn Instance, id: InstanceId) -> Option<&mut dyn Instance> {
    if instance.id() == id {
        return Some(instance);
    }
    if let Some(lookup) = instance.__instance_data().lookup() {
        let mut instance = lookup.descendant(instance.id(), id)?;
        // The subtree check confines the pointer to descendants owned by this borrow.
        return Some(unsafe { instance.as_mut() });
    }
    for child in instance.children_mut() {
        if let Some(found) = resolve_instance_mut(child, id) {
            return Some(found);
        }
    }
    None
}

fn find_child_mut<'a, T: Instance>(
    instance: &'a mut dyn Instance,
    name: &str,
) -> Option<(InstanceId, &'a mut T)> {
    for child in instance.children_mut() {
        let matches = child
            .downcast_ref::<T>()
            .is_some_and(|child| child.name() == name);
        if matches {
            let id = child.id();
            return Some((id, child.downcast_mut::<T>()?));
        }
        if let Some(found) = find_child_mut::<T>(child, name) {
            return Some(found);
        }
    }
    None
}

/// Typed access to an [`Instance`]'s custom metadata.
///
/// Import this trait alongside [`Instance`] to use them:
///
/// ```
/// use trscene::{Instance, InstanceAttributes, Part};
///
/// let part = Part::new()
///     .with_typed_attribute("tags", vec!["wood", "breakable"])
///     .unwrap();
/// assert_eq!(
///     part.get_typed_attribute::<Vec<String>>("tags").unwrap().unwrap(),
///     vec!["wood", "breakable"],
/// );
/// ```
pub trait InstanceAttributes {
    /// Stores any serializable value as custom metadata under `name`,
    /// overwriting any previous value.
    ///
    /// Serialization failures (for example, a map with non-string keys) are
    /// reported without modifying the stored attributes.
    fn with_typed_attribute<T>(
        self,
        name: impl Into<String>,
        value: T,
    ) -> Result<Self, serde_json::Error>
    where
        Self: Sized,
        T: Serialize;

    /// Reads back custom metadata previously stored with
    /// [`Instance::with_attribute`] or [`with_typed_attribute`](Self::with_typed_attribute).
    ///
    /// Returns `None` when no attribute is stored under `name`. A stored value
    /// that does not match `T` yields `Some(Err(_))` instead.
    fn get_typed_attribute<T>(&self, name: &str) -> Option<Result<T, serde_json::Error>>
    where
        T: DeserializeOwned;
}

impl<T: Instance> InstanceAttributes for T {
    fn with_typed_attribute<U>(
        self,
        name: impl Into<String>,
        value: U,
    ) -> Result<Self, serde_json::Error>
    where
        Self: Sized,
        U: Serialize,
    {
        let value = serde_json::to_value(value)?;
        Ok(self.with_attribute(name, value))
    }

    fn get_typed_attribute<U>(&self, name: &str) -> Option<Result<U, serde_json::Error>>
    where
        U: DeserializeOwned,
    {
        read_typed_attribute(self, name)
    }
}

impl InstanceAttributes for dyn Instance {
    fn get_typed_attribute<T>(&self, name: &str) -> Option<Result<T, serde_json::Error>>
    where
        T: DeserializeOwned,
    {
        read_typed_attribute(self, name)
    }
}

fn read_typed_attribute<T>(
    instance: &dyn Instance,
    name: &str,
) -> Option<Result<T, serde_json::Error>>
where
    T: DeserializeOwned,
{
    instance
        .get_attribute(name)
        .map(|value| serde_json::from_value(value.clone()))
}

/// Mutable iterator over an instance's direct children.
pub struct ChildrenMut<'a> {
    inner: std::slice::IterMut<'a, Box<dyn Instance>>,
}

impl<'a> ChildrenMut<'a> {
    fn new(inner: std::slice::IterMut<'a, Box<dyn Instance>>) -> Self {
        Self { inner }
    }
}

impl<'a> Iterator for ChildrenMut<'a> {
    type Item = &'a mut dyn Instance;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(Box::as_mut)
    }
}

impl DoubleEndedIterator for ChildrenMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(Box::as_mut)
    }
}

/// Depth-first iterator over an instance's descendants.
pub struct Descendants<'a> {
    current: std::slice::Iter<'a, Box<dyn Instance>>,
    parents: Vec<std::slice::Iter<'a, Box<dyn Instance>>>,
}

impl<'a> Descendants<'a> {
    fn new(children: &'a [Box<dyn Instance>]) -> Self {
        Self {
            current: children.iter(),
            parents: Vec::new(),
        }
    }
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a dyn Instance;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(instance) = self.current.next() {
                let instance = instance.as_ref();
                let children = instance.children();
                if !children.is_empty() {
                    self.parents
                        .push(std::mem::replace(&mut self.current, children.iter()));
                }
                return Some(instance);
            }
            self.current = self.parents.pop()?;
        }
    }
}

/// The object-safe portion used to clone heterogeneous instance collections.
pub trait InstanceClone {
    /// Clones an instance, or returns `None` when cloning is unsupported.
    fn try_clone_box(&self) -> Option<Box<dyn Instance>>;

    /// Clones an instance behind a trait object.
    ///
    /// # Panics
    /// Panics when this instance does not support cloning.
    fn clone_box(&self) -> Box<dyn Instance> {
        self.try_clone_box()
            .expect("instance does not support cloning")
    }
}

impl<T> InstanceClone for T
where
    T: Instance + Clone + 'static,
{
    fn try_clone_box(&self) -> Option<Box<dyn Instance>> {
        Some(Box::new(self.clone()))
    }
}

impl Clone for Box<dyn Instance> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl dyn Instance {
    /// Tests whether this instance is a `T`.
    pub fn is<T: Any>(&self) -> bool {
        self.as_any().is::<T>()
    }

    /// Downcasts this instance by reference.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }

    /// Downcasts this instance by mutable reference.
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }

    /// Downcasts an owned instance.
    pub fn downcast<T: Any>(self: Box<Self>) -> Result<Box<T>, Box<Self>> {
        if self.is::<T>() {
            let raw = Box::into_raw(self);
            // The type check above guarantees that this trait object contains a T.
            unsafe { Ok(Box::from_raw(raw as *mut T)) }
        } else {
            Err(self)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, Camera, Part, PartShape, Workspace};

    #[test]
    fn built_in_objects_implement_instance_and_support_downcasting() {
        fn assert_instance<T: Instance>() {}

        assert_instance::<Workspace>();
        assert_instance::<BasePart>();
        assert_instance::<Camera>();
        assert_instance::<Part>();
        assert_eq!(Workspace::new().name(), "Workspace");
        assert_eq!(BasePart::new().with_name("base").name(), "base");
        assert_eq!(Camera::default().name(), "Camera");

        let instance: Box<dyn Instance> = Box::new(BasePart::new().with_name("base"));
        assert!(instance.is::<BasePart>());
        assert!(!instance.is::<Part>());
        assert_eq!(instance.downcast_ref::<BasePart>().unwrap().name(), "base");
        let instance = match instance.downcast::<BasePart>() {
            Ok(instance) => instance,
            Err(_) => panic!("expected a BasePart"),
        };
        let instance = instance.with_name("renamed");
        assert_eq!(instance.name(), "renamed");
    }

    #[test]
    fn every_instance_can_own_a_nested_instance_tree() {
        let mut model = BasePart::new().with_name("model");
        let part_id = model.add_child(Part::new().with_shape(PartShape::Ball).with_name("part"));
        let camera_id = Camera::default().set_parent(&mut model);
        let model_id = model.id();

        assert_eq!(
            model.find_first_child::<Part>("part").map(|(id, _)| id),
            Some(part_id)
        );
        assert_eq!(
            model.find_first_child::<Camera>("Camera").map(|(id, _)| id),
            Some(camera_id)
        );
        assert_eq!(model.children().len(), 2);
        assert_eq!(model.children()[0].id(), part_id);
        assert_eq!(model.children()[1].id(), camera_id);
        assert_eq!(model.children()[0].parent(), Some(model_id));

        let mut workspace = Workspace::new();
        model.set_parent(&mut workspace);
        assert_eq!(workspace.children().len(), 1);
        assert_eq!(
            workspace
                .view::<dyn Instance>(model_id)
                .unwrap()
                .class_name(),
            "BasePart"
        );
        assert_eq!(
            workspace.view::<dyn Instance>(part_id).unwrap().parent(),
            Some(model_id)
        );
        assert!(
            workspace
                .view::<dyn Instance>(camera_id)
                .unwrap()
                .is::<Camera>()
        );
        assert_eq!(workspace.get_all::<Part>().count(), 1);
        assert_eq!(workspace.instances().count(), 3);
        assert_eq!(
            workspace
                .instances()
                .map(|instance| instance.name())
                .collect::<Vec<_>>(),
            ["model", "part", "Camera"]
        );
    }

    #[test]
    fn mutable_children_are_iterated_without_exposing_the_backing_slice() {
        let mut workspace = Workspace::new();
        let first_id = Part::new().with_name("first").set_parent(&mut workspace);
        let second_id = Part::new().with_name("second").set_parent(&mut workspace);

        {
            let mut children = workspace.children_mut();
            assert_eq!(children.next().expect("first child").id(), first_id);
            assert_eq!(children.next_back().expect("second child").id(), second_id);
            assert!(children.next().is_none());
        }

        assert!(workspace.remove_child(first_id));
        assert!(workspace.view::<dyn Instance>(second_id).is_some());
    }

    #[test]
    fn dropped_instance_ids_get_a_new_generation() {
        let old_id = Part::new().id();
        let new_id = Part::new().id();

        assert_ne!(old_id, new_id);
    }

    #[test]
    fn attributes_store_and_remove_json_values() {
        let mut part = Part::new().with_name("crate");
        assert!(part.attributes().is_empty());
        assert!(!part.has_attribute("health"));
        assert_eq!(part.get_attribute("health"), None);

        part = part
            .with_attribute("health", serde_json::json!(100))
            .with_attribute(
                "tags",
                serde_json::json!({"material": "wood", "breakable": true}),
            );
        assert!(part.has_attribute("health"));
        assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(100)));

        // Overwriting replaces the previous value and reports it on removal.
        part = part.with_attribute("health", serde_json::json!(75));
        assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(75)));
        assert_eq!(
            part.attributes().keys().collect::<Vec<_>>(),
            ["health", "tags"]
        );
        assert_eq!(part.remove_attribute("health"), Some(serde_json::json!(75)));
        assert!(!part.has_attribute("health"));
        assert_eq!(part.remove_attribute("health"), None);

        part.clear_attributes();
        assert!(part.attributes().is_empty());
    }

    #[test]
    fn typed_attributes_round_trip_any_serializable_value() {
        let part = Part::new()
            .with_name("crate")
            .with_typed_attribute("tags", vec!["wood", "breakable"])
            .unwrap()
            .with_typed_attribute("health", 100_i64)
            .unwrap();

        assert_eq!(
            part.get_typed_attribute::<Vec<String>>("tags")
                .unwrap()
                .unwrap(),
            vec!["wood", "breakable"],
        );
        assert_eq!(
            part.get_typed_attribute::<i64>("health").unwrap().unwrap(),
            100
        );
        assert!(part.get_typed_attribute::<i64>("missing").is_none());
        assert!(
            part.get_typed_attribute::<Vec<String>>("health")
                .unwrap()
                .is_err(),
            "a type mismatch should report an error instead of panicking"
        );
    }

    #[test]
    fn attributes_are_available_behind_trait_objects() {
        let instance: Box<dyn Instance> = Box::new(
            Part::new()
                .with_name("crate")
                .with_attribute("health", serde_json::json!(100))
                .with_typed_attribute("tags", vec!["wood".to_owned()])
                .unwrap(),
        );
        assert_eq!(
            instance.get_attribute("health"),
            Some(&serde_json::json!(100))
        );
        assert!(instance.has_attribute("health"));
        assert_eq!(instance.attributes().len(), 2);

        assert_eq!(
            instance
                .get_typed_attribute::<Vec<String>>("tags")
                .unwrap()
                .unwrap(),
            vec!["wood"],
        );

        let mut workspace = Workspace::new();
        let id = instance.id();
        let child_id = Part::new()
            .with_name("child")
            .with_attribute("health", serde_json::json!(42))
            .set_parent(&mut workspace);
        assert!(workspace.view::<dyn Instance>(id).is_none());
        assert_eq!(
            workspace
                .view::<dyn Instance>(child_id)
                .unwrap()
                .get_attribute("health"),
            Some(&serde_json::json!(42))
        );
    }

    #[test]
    fn attributes_are_independent_per_instance_and_survive_cloning() {
        let mut parent = BasePart::new()
            .with_name("parent")
            .with_attribute("role", serde_json::json!("parent"));
        let child_id = parent.add_child(
            Part::new()
                .with_name("child")
                .with_attribute("role", serde_json::json!("child")),
        );
        let child = parent.view_mut::<dyn Instance>(child_id).unwrap();
        assert_eq!(
            child.get_attribute("role"),
            Some(&serde_json::json!("child"))
        );

        let cloned: BasePart = parent.clone();
        assert_eq!(
            cloned.get_attribute("role"),
            Some(&serde_json::json!("parent"))
        );
        let cloned_child_id = cloned.children()[0].id();
        assert_eq!(
            cloned
                .view::<dyn Instance>(cloned_child_id)
                .unwrap()
                .get_attribute("role"),
            Some(&serde_json::json!("child"))
        );

        let mut workspace = Workspace::new();
        parent
            .with_attribute("saved", serde_json::json!(true))
            .set_parent(&mut workspace);
        let cloned_workspace = workspace.clone();
        let cloned_parent = cloned_workspace
            .instances()
            .find(|instance| instance.name() == "parent")
            .unwrap();
        assert_eq!(
            cloned_parent.get_attribute("saved"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            cloned_parent.get_attribute("role"),
            Some(&serde_json::json!("parent"))
        );
    }
}
