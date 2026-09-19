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

use crate::glam::{EulerRot, Mat4, Quat, Vec3};
mod camera;
mod light;
#[cfg(feature = "meshpart")]
mod meshpart;
mod outline;
mod part;
mod workspace;
pub use camera::*;
pub use light::*;
#[cfg(feature = "meshpart")]
pub use meshpart::*;
pub use outline::*;
pub use part::*;
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
pub struct InstanceLookup {
    instances: RefCell<InstanceMap<NonNull<dyn Instance>>>,
    root: Cell<Option<(InstanceId, NonNull<InstanceData>)>>,
}

impl InstanceLookup {
    /// Registers an instance pointer in this lookup index.
    #[doc(hidden)]
    pub fn register(&self, instance: &mut dyn Instance) {
        self.instances
            .borrow_mut()
            .insert(instance.id(), NonNull::from(instance));
    }

    pub(crate) fn unregister(&self, id: InstanceId) {
        self.instances.borrow_mut().remove(&id);
    }

    pub(crate) fn get(&self, id: InstanceId) -> Option<NonNull<dyn Instance>> {
        self.instances.borrow().get(&id).copied()
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

/// Implements the common [`Instance`] plumbing for a type backed by
/// [`InstanceData`]. The field paths provide the type-specific name and
/// storage locations.
#[macro_export]
macro_rules! impl_instance {
    (
        $type:ty,
        class_name = $class_name:literal,
        data = $data:ident $(.$data_tail:ident)* $(,)?
    ) => {
        impl $crate::Instance for $type {
            fn class_name(&self) -> &'static str {
                $class_name
            }

            fn name(&self) -> &str {
                self.$data $(.$data_tail)*.name()
            }

            fn with_name(mut self, name: impl Into<String>) -> Self {
                self.$data $(.$data_tail)*.set_name(name.into());
                self
            }

            fn id(&self) -> $crate::InstanceId {
                self.$data $(.$data_tail)*.id()
            }

            fn parent(&self) -> Option<$crate::InstanceId> {
                self.$data $(.$data_tail)*.parent()
            }

            fn children(&self) -> &[Box<dyn $crate::Instance>] {
                self.$data $(.$data_tail)*.children()
            }

            fn children_mut(&mut self) -> $crate::ChildrenMut<'_> {
                self.$data $(.$data_tail)*.children_mut()
            }

            fn remove_child(&mut self, id: $crate::InstanceId) -> bool {
                self.$data $(.$data_tail)*.remove_child(id)
            }

            fn add_child_box(
                &mut self,
                child: Box<dyn $crate::Instance>,
            ) -> $crate::InstanceId {
                self.$data $(.$data_tail)*.add_child(child)
            }

            fn set_instance_parent(&mut self, parent: Option<$crate::InstanceId>) {
                self.$data $(.$data_tail)*.set_parent(parent);
            }

            fn sibling_index(&self) -> usize {
                self.$data $(.$data_tail)*.sibling_index()
            }

            fn set_sibling_index(&mut self, index: usize) {
                self.$data $(.$data_tail)*.set_sibling_index(index);
            }

            fn with_attribute(
                mut self,
                name: impl Into<String>,
                value: serde_json::Value,
            ) -> Self {
                self.$data $(.$data_tail)*.set_attribute(name.into(), value);
                self
            }

            fn get_attribute(&self, name: &str) -> Option<&serde_json::Value> {
                self.$data $(.$data_tail)*.get_attribute(name)
            }

            fn attributes(&self) -> &$crate::Attributes {
                self.$data $(.$data_tail)*.attributes()
            }

            fn remove_attribute(&mut self, name: &str) -> Option<serde_json::Value> {
                self.$data $(.$data_tail)*.remove_attribute(name)
            }

            fn clear_attributes(&mut self) {
                self.$data $(.$data_tail)*.clear_attributes();
            }

            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }

            fn set_instance_lookup(
                &mut self,
                lookup: Option<::std::rc::Rc<$crate::InstanceLookup>>,
            ) {
                if let Some(lookup) = &lookup {
                    lookup.register(self);
                } else if let Some(lookup) = self.instance_lookup() {
                    lookup.unregister(self.id());
                }

                self.$data $(.$data_tail)*.set_lookup(lookup.as_ref());

                for child in self.children_mut() {
                    child.set_instance_lookup(lookup.clone());
                }
            }

            fn instance_lookup(&self) -> Option<::std::rc::Rc<$crate::InstanceLookup>> {
                self.$data $(.$data_tail)*.lookup()
            }
        }
    };
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
pub(crate) struct InstanceData {
    name: String,
    id: InstanceId,
    parent: Option<InstanceId>,
    children: Vec<Box<dyn Instance>>,
    sibling_index: usize,
    lookup: Weak<InstanceLookup>,
    attributes: Attributes,
}

impl InstanceData {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: InstanceId::new(),
            parent: None,
            children: Vec::new(),
            sibling_index: usize::MAX,
            lookup: Weak::new(),
            attributes: Attributes::new(),
        }
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

    pub(crate) fn children_mut(&mut self) -> ChildrenMut<'_> {
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
        child.set_instance_parent(Some(self.id));
        child.set_sibling_index(self.children.len());
        let child_id = child.id();
        self.children.push(child);
        if let Some(lookup) = self.lookup() {
            self.children
                .last_mut()
                .expect("just pushed child")
                .set_instance_lookup(Some(lookup));
        }
        child_id
    }

    pub(crate) fn remove_child(&mut self, id: InstanceId) -> bool {
        let indexed_position = self.lookup().and_then(|lookup| {
            let instance = lookup.get(id)?;
            // Registered instances remain boxed at stable addresses while parented.
            Some(unsafe { instance.as_ref().sibling_index() })
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
        let mut child = self.children.swap_remove(index);
        if let Some(moved_child) = self.children.get_mut(index) {
            moved_child.set_sibling_index(index);
        }
        child.set_instance_parent(None);
        child.set_sibling_index(usize::MAX);
        child.set_instance_lookup(None);
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

/// A node in the object hierarchy.
///
/// Implementors expose their concrete type through [`Any`], which allows a
/// heterogeneous collection such as [`Workspace`] to recover typed instances
/// without requiring callers to maintain parallel type-specific collections.
pub trait Instance: Any + Debug + InstanceClone {
    /// The concrete class name of this instance.
    fn class_name(&self) -> &'static str;

    /// The display name of this instance.
    ///
    /// This is the class name e.g. `"Part"`, `"PointLight"` if
    /// not explicitly set.
    fn name(&self) -> &str;

    /// Returns this instance with its display name set.
    fn with_name(self, name: impl Into<String>) -> Self
    where
        Self: Sized;

    /// Returns this instance's stable identifier.
    fn id(&self) -> InstanceId;

    /// Stores a custom metadata value under `name`, overwriting any previous
    /// value.
    ///
    /// This is arbitrary per-instance data that the engine itself ignores. Any
    /// JSON value works, so nested objects and arrays are supported. Prefer
    /// [`InstanceAttributes::with_typed_attribute`] to store a serializable
    /// Rust value without building the JSON by hand.
    ///
    /// ```
    /// use terrarium::{Instance, Part};
    ///
    /// let part = Part::new()
    ///     .with_name("crate")
    ///     .with_attribute("health", serde_json::json!(100));
    /// assert_eq!(part.get_attribute("health"), Some(&serde_json::json!(100)));
    /// ```
    fn with_attribute(self, name: impl Into<String>, value: serde_json::Value) -> Self
    where
        Self: Sized;

    /// Returns the custom metadata stored under `name`, if any.
    fn get_attribute(&self, name: &str) -> Option<&serde_json::Value>;

    /// Returns all custom metadata in sorted key order.
    fn attributes(&self) -> &Attributes;

    /// Reports whether custom metadata is stored under `name`.
    fn has_attribute(&self, name: &str) -> bool {
        self.get_attribute(name).is_some()
    }

    /// Removes the custom metadata stored under `name`, returning it when present.
    fn remove_attribute(&mut self, name: &str) -> Option<serde_json::Value>;

    /// Removes all custom metadata from this instance.
    fn clear_attributes(&mut self);

    /// Returns the parent identifier, or `None` for an isolated instance.
    fn parent(&self) -> Option<InstanceId>;

    /// Returns this instance's children in insertion order.
    fn children(&self) -> &[Box<dyn Instance>];

    /// Iterates over mutable access to this instance's children.
    fn children_mut(&mut self) -> ChildrenMut<'_>;

    /// Removes a direct child by its stable identifier.
    #[doc(hidden)]
    fn remove_child(&mut self, id: InstanceId) -> bool {
        let _ = id;
        false
    }

    /// Removes this instance from its parent and destroys its descendants.
    ///
    /// Returns `true` when the instance was parented in a workspace. An
    /// isolated instance is already owned by its caller and is destroyed by
    /// dropping that value instead.
    ///
    /// This must be the last operation performed through this instance
    /// reference when it returns `true`.
    fn destroy(&mut self) -> bool {
        let Some(parent_id) = self.parent() else {
            return false;
        };
        let id = self.id();
        let Some(lookup) = self.instance_lookup() else {
            return false;
        };

        lookup.remove_child(parent_id, id)
    }

    /// Adds an owned child to this instance.
    fn add_child_box(&mut self, child: Box<dyn Instance>) -> InstanceId;

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

    /// Configures an isolated child, then adds it and returns a mutable reference to it.
    ///
    /// The concrete type is known statically, so no downcasting is needed.
    /// The reference borrows the parent, so copy out the [`InstanceId`] with
    /// [`Instance::id`] if the handle must outlive the borrow.
    fn add_child_with_ref<T, F>(&mut self, mut child: T, configure: F) -> &mut T
    where
        Self: Sized,
        T: Instance,
        F: FnOnce(&mut T),
    {
        configure(&mut child);
        self.add_child_ref(child)
    }

    /// Sets the internal parent link while a parent takes ownership of this instance.
    fn set_instance_parent(&mut self, parent: Option<InstanceId>);

    /// Returns this instance's position in its parent's child storage.
    #[doc(hidden)]
    fn sibling_index(&self) -> usize {
        usize::MAX
    }

    /// Updates this instance's position in its parent's child storage.
    #[doc(hidden)]
    fn set_sibling_index(&mut self, index: usize) {
        let _ = index;
    }

    /// Returns this value as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Returns this value as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Internal hook used to register an instance subtree in its workspace index.
    #[doc(hidden)]
    fn set_instance_lookup(&mut self, lookup: Option<Rc<InstanceLookup>>) {
        for child in self.children_mut() {
            child.set_instance_lookup(lookup.clone());
        }
    }

    /// Returns the workspace lookup associated with this instance.
    #[doc(hidden)]
    fn instance_lookup(&self) -> Option<Rc<InstanceLookup>> {
        None
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

    /// Finds a descendant by ID.
    fn find_descendant(&self, id: InstanceId) -> Option<&dyn Instance> {
        for child in self.children() {
            if child.id() == id {
                return Some(child.as_ref());
            }
            if let Some(found) = child.find_descendant(id) {
                return Some(found);
            }
        }
        None
    }

    /// Finds a mutable descendant by ID.
    fn find_descendant_mut(&mut self, id: InstanceId) -> Option<&mut dyn Instance> {
        for child in self.children_mut() {
            if child.id() == id {
                return Some(child);
            }
            if let Some(found) = child.find_descendant_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// Finds the first descendant of type `T` with `name` in depth-first order.
    fn find_first_child<T: Instance>(&mut self, name: &str) -> Option<(InstanceId, &mut T)>
    where
        Self: Sized,
    {
        find_child_mut(self, name)
    }
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
/// use terrarium::{Instance, InstanceAttributes, Part};
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
    /// Clones an instance behind a trait object.
    fn clone_box(&self) -> Box<dyn Instance>;
}

impl<T> InstanceClone for T
where
    T: Instance + Clone + 'static,
{
    fn clone_box(&self) -> Box<dyn Instance> {
        Box::new(self.clone())
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

/// An object that has a physical location in the world.
#[derive(Clone, Debug)]
pub struct PVInstance {
    pivot: Mat4,
}

impl Default for PVInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl PVInstance {
    /// Creates an instance at the identity transform.
    pub fn new() -> Self {
        Self {
            pivot: Mat4::IDENTITY,
        }
    }

    /// Creates a new instance from a world-space transform.
    pub fn from_world_transform(transform: Mat4) -> Self {
        Self { pivot: transform }
    }
}

/// Access to the underlying [`PVInstance`].
pub trait HasPVInstance {
    /// Returns shared access to the underlying [`PVInstance`].
    fn pv(&self) -> &PVInstance;

    /// Returns mutable access to the underlying [`PVInstance`].
    fn pv_mut(&mut self) -> &mut PVInstance;

    /// The world-space transform of the instance's pivot.
    fn pivot(&self) -> Mat4 {
        self.pv().pivot
    }

    /// Returns the translation component of the pivot.
    fn position(&self) -> Vec3 {
        self.pv().pivot.w_axis.truncate()
    }

    /// Returns XYZ Euler orientation angles in degrees.
    fn orientation(&self) -> Vec3 {
        let (_, rotation, _) = self.pv().pivot.to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);

        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
    }

    /// Returns the normalized world-space direction of local `-Z`.
    fn forward(&self) -> Vec3 {
        self.pivot()
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or_zero()
    }

    /// Transforms the [`PVInstance`] along with all of its descendant [`PVInstance`]s such that the pivot is now located at the specified transform.
    fn with_pivot(mut self, pivot: Mat4) -> Self
    where
        Self: Sized,
    {
        self.pv_mut().pivot = pivot;
        self
    }

    /// Replaces the translation while preserving the current rotation.
    fn with_position(mut self, position: Vec3) -> Self
    where
        Self: Sized,
    {
        let (_, rotation, _) = self.pv().pivot.to_scale_rotation_translation();
        self.pv_mut().pivot = Mat4::from_rotation_translation(rotation, position);
        self
    }

    /// Replaces the XYZ Euler orientation in degrees while preserving position.
    fn with_orientation(mut self, orientation: Vec3) -> Self
    where
        Self: Sized,
    {
        let position = self.position();
        self.pv_mut().pivot = Mat4::from_rotation_translation(
            Quat::from_euler(
                EulerRot::XYZ,
                orientation.x.to_radians(),
                orientation.y.to_radians(),
                orientation.z.to_radians(),
            ),
            position,
        );
        self
    }
}

impl HasPVInstance for PVInstance {
    fn pv(&self) -> &PVInstance {
        self
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        self
    }
}

impl<T: HasPVInstance + ?Sized> HasPVInstance for &mut T {
    fn pv(&self) -> &PVInstance {
        (**self).pv()
    }

    fn pv_mut(&mut self) -> &mut PVInstance {
        (**self).pv_mut()
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
            workspace.instance(model_id).unwrap().class_name(),
            "BasePart"
        );
        assert_eq!(
            workspace.instance(part_id).unwrap().parent(),
            Some(model_id)
        );
        assert!(workspace.instance(camera_id).unwrap().is::<Camera>());
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
        assert!(workspace.instance(second_id).is_some());
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
        assert!(workspace.instance(id).is_none());
        assert_eq!(
            workspace
                .instance(child_id)
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
        let child = parent.find_descendant_mut(child_id).unwrap();
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
                .find_descendant(cloned_child_id)
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
