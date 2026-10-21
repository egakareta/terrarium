//! Receiver-preserving instance views and generated composition metadata.

use std::any::{Any, TypeId};

use super::Instance;

/// Shared and mutable access to a composed storage type.
///
/// Implementations are generated from composition fields by [`crate::instance!`].
pub trait Project<T: ?Sized> {
    /// Returns the storage provided by this receiver.
    fn project(&self) -> &T;
    /// Returns mutable storage provided by this receiver.
    fn project_mut(&mut self) -> &mut T;
    /// Returns the complete mutable receiver when this projection belongs to an instance.
    #[doc(hidden)]
    fn owner_mut(&mut self) -> Option<&mut dyn Instance> {
        None
    }
}

impl<T: ?Sized, R: Project<T> + ?Sized> Project<T> for &mut R {
    fn project(&self) -> &T {
        (**self).project()
    }
    fn owner_mut(&mut self) -> Option<&mut dyn Instance> {
        (**self).owner_mut()
    }
    fn project_mut(&mut self) -> &mut T {
        (**self).project_mut()
    }
}

/// Access to the owning instance, including through a borrowed API view.
pub trait InstanceReceiver {
    /// Returns the complete instance, preserving its concrete identity.
    fn as_instance(&self) -> &dyn Instance;
    /// Returns the complete mutable instance.
    fn as_instance_mut(&mut self) -> &mut dyn Instance;
}

impl<T: InstanceReceiver + ?Sized> InstanceReceiver for &mut T {
    fn as_instance(&self) -> &dyn Instance {
        (**self).as_instance()
    }
    fn as_instance_mut(&mut self) -> &mut dyn Instance {
        (**self).as_instance_mut()
    }
}

/// A generated class or capability and its receiver-preserving API.
pub trait InstanceType: 'static {
    /// The trait object exposed by a runtime view.
    type Api: Instance + ?Sized + 'static;
    /// Returns this type's composition graph node.
    fn descriptor() -> &'static InstanceClass;
}

/// One node in the generated composition graph.
pub struct InstanceClass {
    /// The declared Rust type name.
    pub name: &'static str,
    /// Returns the concrete Rust type identifier.
    pub type_id: fn() -> TypeId,
    /// Returns descriptors for the directly composed types.
    pub bases: &'static [fn() -> &'static InstanceClass],
}

impl InstanceClass {
    /// Reports whether this class provides `T`, directly or transitively.
    pub fn is_a<T: InstanceType>(&self) -> bool {
        (self.type_id)() == TypeId::of::<T>() || self.bases.iter().any(|base| base().is_a::<T>())
    }
}

/// Safe adapters from an exact concrete receiver to one of its generated APIs.
#[doc(hidden)]
pub struct InstanceQuery<T: ?Sized + 'static> {
    /// Borrows an API from its concrete receiver.
    pub shared: for<'a> fn(&'a dyn Any) -> &'a T,
    /// Mutably borrows an API from its concrete receiver.
    pub mutable: for<'a> fn(&'a mut dyn Any) -> &'a mut T,
}

/// Selects inspector support for a concrete generated property type.
#[doc(hidden)]
#[cfg(feature = "devtools")]
pub struct __InstancePropertyProbe<T>(
    /// Carries the property type without storing a value.
    pub std::marker::PhantomData<T>,
);

/// Inspector operations selected at the concrete macro expansion site.
#[doc(hidden)]
#[cfg(feature = "devtools")]
pub trait __InspectInstanceProperty<T> {
    /// Returns editable fields when the property supports inspection.
    fn properties(self, value: &T) -> Vec<crate::devtools::Property>;
    /// Applies an edit when the property supports inspection.
    fn set_property(
        self,
        value: &mut T,
        path: &str,
        edit: crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError>;
}

#[cfg(feature = "devtools")]
impl<T> __InspectInstanceProperty<T> for &__InstancePropertyProbe<T> {
    fn properties(self, _value: &T) -> Vec<crate::devtools::Property> {
        Vec::new()
    }
    fn set_property(
        self,
        _value: &mut T,
        _path: &str,
        _edit: crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        Err(crate::devtools::PropertyError::NotFound)
    }
}

#[cfg(feature = "devtools")]
impl<T: crate::devtools::Inspect> __InspectInstanceProperty<T> for &&__InstancePropertyProbe<T> {
    fn properties(self, value: &T) -> Vec<crate::devtools::Property> {
        crate::devtools::Inspect::properties(value)
    }
    fn set_property(
        self,
        value: &mut T,
        path: &str,
        edit: crate::devtools::PropertyValue,
    ) -> Result<(), crate::devtools::PropertyError> {
        crate::devtools::Inspect::set_property(value, path, edit)
    }
}

impl dyn Instance {
    /// Borrows an inherited API while preserving this instance's identity.
    pub fn view<T: InstanceType>(&self) -> Option<&T::Api> {
        let query = self
            .query(TypeId::of::<T>())?
            .downcast_ref::<InstanceQuery<T::Api>>()?;
        Some((query.shared)(self.as_any()))
    }

    /// Mutably borrows an inherited API with the owning receiver's overrides.
    pub fn view_mut<T: InstanceType>(&mut self) -> Option<&mut T::Api> {
        let query = self
            .query(TypeId::of::<T>())?
            .downcast_ref::<InstanceQuery<T::Api>>()?;
        Some((query.mutable)(self.as_any_mut()))
    }

    /// Tests generated class ancestry without changing exact downcast semantics.
    pub fn is_a<T: InstanceType>(&self) -> bool {
        self.query(TypeId::of::<T>()).is_some()
    }
}
