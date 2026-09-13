use std::{
    any::Any,
    fmt::Debug,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::glam::{EulerRot, Mat4, Quat, Vec3};

static NEXT_INSTANCE_ID: AtomicUsize = AtomicUsize::new(0);

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
        impl $type {
            /// Moves this isolated instance into `parent` and returns its stable identifier.
            pub fn set_parent(
                self,
                parent: &mut dyn $crate::Instance,
            ) -> $crate::InstanceId {
                let id = $crate::Instance::id(&self);
                parent.add_child_box(Box::new(self));
                id
            }

            /// Adds an owned child and returns the child's stable identifier.
            pub fn add_child<T>(&mut self, child: T) -> $crate::InstanceId
            where
                T: $crate::Instance,
            {
                <$type as $crate::Instance>::add_child_box(self, Box::new(child))
            }

            /// Configures a child before adding it and returns the child's stable identifier.
            pub fn add_child_with<T, F>(
                &mut self,
                mut child: T,
                configure: F,
            ) -> $crate::InstanceId
            where
                T: $crate::Instance,
                F: FnOnce(&mut T),
            {
                configure(&mut child);
                self.add_child(child)
            }
        }

        impl $crate::Instance for $type {
            fn class_name(&self) -> &'static str {
                $class_name
            }

            fn name(&self) -> &str {
                self.$data $(.$data_tail)*.name()
            }

            fn set_name(&mut self, name: String) {
                self.$data $(.$data_tail)*.set_name(name);
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

            fn children_mut(&mut self) -> &mut [Box<dyn $crate::Instance>] {
                self.$data $(.$data_tail)*.children_mut()
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

            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
                self
            }
        }
    };
}

/// Stable identifier for an [`Instance`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InstanceId(usize);

impl InstanceId {
    fn new() -> Self {
        Self(NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug)]
pub(crate) struct InstanceData {
    name: String,
    id: InstanceId,
    parent: Option<InstanceId>,
    children: Vec<Box<dyn Instance>>,
}

impl InstanceData {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: InstanceId::new(),
            parent: None,
            children: Vec::new(),
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

    pub(crate) fn children(&self) -> &[Box<dyn Instance>] {
        &self.children
    }

    pub(crate) fn children_mut(&mut self) -> &mut [Box<dyn Instance>] {
        &mut self.children
    }

    pub(crate) fn add_child(&mut self, mut child: Box<dyn Instance>) -> InstanceId {
        let id = child.id();
        child.set_instance_parent(Some(self.id));
        self.children.push(child);
        id
    }
}

impl Clone for InstanceData {
    fn clone(&self) -> Self {
        let mut cloned = Self::new(self.name.clone());
        for child in &self.children {
            let mut child = child.clone();
            child.set_instance_parent(Some(cloned.id));
            cloned.children.push(child);
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
    fn name(&self) -> &str;

    /// Changes the display name of this instance.
    fn set_name(&mut self, name: String);

    /// Returns this instance's stable identifier.
    fn id(&self) -> InstanceId;

    /// Returns the parent identifier, or `None` for an isolated instance.
    fn parent(&self) -> Option<InstanceId>;

    /// Returns this instance's children in insertion order.
    fn children(&self) -> &[Box<dyn Instance>];

    /// Returns mutable access to this instance's children.
    fn children_mut(&mut self) -> &mut [Box<dyn Instance>];

    /// Adds an owned child to this instance.
    fn add_child_box(&mut self, child: Box<dyn Instance>) -> InstanceId;

    /// Configures an isolated child, then adds it and returns its identifier.
    fn add_child_with<T, F>(&mut self, mut child: T, configure: F) -> InstanceId
    where
        Self: Sized,
        T: Instance,
        F: FnOnce(&mut T),
    {
        configure(&mut child);
        self.add_child_box(Box::new(child))
    }

    /// Sets the internal parent link while a parent takes ownership of this instance.
    fn set_instance_parent(&mut self, parent: Option<InstanceId>);

    /// Returns this value as [`Any`] for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Returns this value as mutable [`Any`] for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;

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
                return Some(child.as_mut());
            }
            if let Some(found) = child.find_descendant_mut(id) {
                return Some(found);
            }
        }
        None
    }
}

/// Depth-first iterator over an instance's descendants.
pub struct Descendants<'a> {
    pending: Vec<&'a dyn Instance>,
}

impl<'a> Descendants<'a> {
    fn new(children: &'a [Box<dyn Instance>]) -> Self {
        Self {
            pending: children.iter().rev().map(Box::as_ref).collect(),
        }
    }
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a dyn Instance;

    fn next(&mut self) -> Option<Self::Item> {
        let instance = self.pending.pop()?;
        self.pending
            .extend(instance.children().iter().rev().map(Box::as_ref));
        Some(instance)
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

    /// The world-space transform of the instance's pivot.
    pub fn pivot(&self) -> Mat4 {
        self.pivot
    }

    /// Transforms the [`PVInstance`] along with all of its descendant [`PVInstance`]s such that the pivot is now located at the specified transform.
    pub fn pivot_to(&mut self, pivot: Mat4) {
        self.pivot = pivot;
    }

    /// Returns the translation component of the pivot.
    pub fn position(&self) -> Vec3 {
        self.pivot.w_axis.truncate()
    }

    /// Replaces the translation while preserving the current rotation.
    pub fn set_position(&mut self, position: Vec3) {
        let (_, rotation, _) = self.pivot.to_scale_rotation_translation();
        self.pivot = Mat4::from_rotation_translation(rotation, position);
    }

    /// Returns XYZ Euler orientation angles in degrees.
    pub fn orientation(&self) -> Vec3 {
        let (_, rotation, _) = self.pivot.to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);

        Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
    }

    /// Replaces the XYZ Euler orientation in degrees while preserving position.
    pub fn set_orientation(&mut self, orientation: Vec3) {
        self.pivot = Mat4::from_rotation_translation(
            Quat::from_euler(
                EulerRot::XYZ,
                orientation.x.to_radians(),
                orientation.y.to_radians(),
                orientation.z.to_radians(),
            ),
            self.position(),
        );
    }

    /// Returns the normalized world-space direction of local `-Z`.
    pub fn forward(&self) -> Vec3 {
        self.pivot()
            .transform_vector3(Vec3::NEG_Z)
            .normalize_or_zero()
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
        assert_eq!(BasePart::new("base").name(), "base");
        assert_eq!(Camera::default().name(), "Camera");

        let mut instance: Box<dyn Instance> = Box::new(BasePart::new("base"));
        assert!(instance.is::<BasePart>());
        assert!(!instance.is::<Part>());
        assert_eq!(instance.downcast_ref::<BasePart>().unwrap().name(), "base");
        instance
            .downcast_mut::<BasePart>()
            .unwrap()
            .set_name("renamed".to_owned());
        assert_eq!(instance.name(), "renamed");

        let instance = match instance.downcast::<BasePart>() {
            Ok(instance) => instance,
            Err(_) => panic!("expected a BasePart"),
        };
        assert_eq!(instance.name(), "renamed");
    }

    #[test]
    fn every_instance_can_own_a_nested_instance_tree() {
        let mut model = BasePart::new("model");
        let part_id = model.add_child_with(Part::new("part"), |part| {
            part.shape = PartShape::Ball;
        });
        let camera_id = Camera::default().set_parent(&mut model);
        let model_id = model.id();

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
    }
}
