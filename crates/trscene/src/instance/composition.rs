//! Receiver-preserving instance views and generated composition metadata.

use std::any::{Any, TypeId};

use super::{Instance, InstanceData, InstanceId};

/// Shared and mutable access to a composed storage type.
///
/// Implementations are generated from composition fields by [`crate::instance!`].
/// Identity storage is excluded from safe projections:
///
/// ```compile_fail,E0277
/// use trscene::{InstanceData, Part, Project};
/// let mut part = Part::new();
/// let _ = <Part as Project<InstanceData>>::project_mut(&mut part);
/// ```
///
/// Direct identity mutation requires an unsafe internal hook:
///
/// ```compile_fail,E0133
/// use trscene::{Part, __InstanceDataAccess};
/// let mut part = Part::new();
/// let _ = part.__instance_data_mut();
/// ```
///
/// Bookkeeping mutators are private even on standalone identity storage:
///
/// ```compile_fail,E0624
/// use trscene::InstanceData;
/// let mut data = InstanceData::default();
/// data.set_lookup(None);
/// ```
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

/// Internal identity storage supplied by generated instances.
///
/// # Safety
/// Both methods must return the same identity provider owned by this receiver.
#[doc(hidden)]
pub unsafe trait __InstanceDataAccess {
    /// Returns this receiver's identity storage.
    fn __instance_data(&self) -> &InstanceData;

    /// Returns mutable identity storage for runtime operations.
    ///
    /// # Safety
    /// The caller must preserve tree registration, parent links, and sibling indices,
    /// and must not move or replace registered storage.
    unsafe fn __instance_data_mut(&mut self) -> &mut InstanceData;
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

    /// Projects an instance into this type's receiver-preserving API.
    #[doc(hidden)]
    fn __view(instance: &dyn Instance) -> Option<&Self::Api> {
        let query = instance
            .query(TypeId::of::<Self>())?
            .downcast_ref::<InstanceQuery<Self::Api>>()?;
        Some((query.shared)(instance.as_any()))
    }

    /// Projects mutable access into this type's receiver-preserving API.
    #[doc(hidden)]
    fn __view_mut(instance: &mut dyn Instance) -> Option<&mut Self::Api> {
        let query = instance
            .query(TypeId::of::<Self>())?
            .downcast_ref::<InstanceQuery<Self::Api>>()?;
        Some((query.mutable)(instance.as_any_mut()))
    }
}

impl InstanceType for dyn Instance {
    type Api = dyn Instance;

    fn descriptor() -> &'static InstanceClass {
        &InstanceClass {
            name: "Instance",
            type_id: TypeId::of::<Self>,
            bases: &[],
        }
    }

    fn __view(instance: &dyn Instance) -> Option<&Self::Api> {
        Some(instance)
    }

    fn __view_mut(instance: &mut dyn Instance) -> Option<&mut Self::Api> {
        Some(instance)
    }
}

/// Typed access to an instance and its descendants, including through trait objects.
pub trait InstanceExt: Instance {
    /// Borrows an instance by ID, preserving its identity and inherited APIs.
    ///
    /// The ID must belong to this instance or one of its descendants. Use
    /// `view::<dyn Instance>` when no specific class or capability is required.
    fn view<T: InstanceType + ?Sized>(&self, id: InstanceId) -> Option<&T::Api> {
        T::__view(super::resolve_instance(self.as_instance(), id)?)
    }

    /// Mutably borrows an instance by ID with its owning receiver's overrides.
    ///
    /// The ID must belong to this instance or one of its descendants.
    fn view_mut<T: InstanceType + ?Sized>(&mut self, id: InstanceId) -> Option<&mut T::Api> {
        T::__view_mut(super::resolve_instance_mut(self.as_instance_mut(), id)?)
    }
}

impl<T: Instance + ?Sized> InstanceExt for T {}

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
    pub fn is_a<T: InstanceType + ?Sized>(&self) -> bool {
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
    /// Tests generated class ancestry without changing exact downcast semantics.
    pub fn is_a<T: InstanceType + ?Sized>(&self) -> bool {
        T::__view(self).is_some()
    }
}
