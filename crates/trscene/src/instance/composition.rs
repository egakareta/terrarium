//! Compose scene instances with generated methods and receiver-preserving views.
//!
//! ```
//! use trscene::{Part, BasePart, Workspace, prelude::*};
//!
//! trscene::instance! {
//!     #[derive(Clone, Debug)]
//!     pub struct SpringPart {
//!         #[extends]
//!         part: Part,
//!         #[property(default = 20.0)]
//!         stiffness: f32,
//!     }
//! }
//!
//! let mut workspace = Workspace::new();
//! let id = workspace.add_child(SpringPart::default().with_stiffness(35.0));
//! workspace.view_mut::<BasePart>(id).unwrap().set_transparency(0.5);
//! assert_eq!(workspace.view::<SpringPart>(id).unwrap().stiffness(), 35.0);
//! assert!(workspace.get::<Part>(id).is_none());
//! ```
//!
//! `#[extends]` chooses the identity provider. Additional `#[compose]` fields
//! contribute capability APIs. Composition is by value and requires concrete
//! types. A component without an identity field can be composed into an instance.
//! Nested public types declare their exported location with
//! `#[instance(path = crate::module)]`; import the parent type and its same-named
//! schema macro together. No central type registry is required.
//!
//! Public receiver methods in the macro's impl become generated API methods.
//! Use `#[r#override(Base::method)]` for inherited behavior. Property builders,
//! other default methods, views, and inspector edits dispatch through these
//! overrides on the complete receiver. `Project` access deliberately exposes
//! embedded storage directly. Owned downcasts continue to require an exact type.
//!
//! An instance cannot contain two identity providers:
//!
//! ```compile_fail
//! use trscene::{Part, Rig};
//! trscene::instance! {
//!     #[derive(Clone, Debug)]
//!     pub struct Ambiguous {
//!         #[extends]
//!         part: Part,
//!         #[compose]
//!         rig: Rig,
//!     }
//! }
//! ```
//!
//! Repeated providers of the same capability are ambiguous:
//!
//! ```compile_fail
//! use trscene::{Part, PVInstance};
//! trscene::instance! {
//!     #[derive(Clone, Debug)]
//!     pub struct Ambiguous {
//!         #[extends]
//!         part: Part,
//!         #[compose]
//!         pivot: PVInstance,
//!     }
//! }
//! ```
//!
//! Overrides must name an existing composed method:
//!
//! ```compile_fail
//! use trscene::Part;
//! trscene::instance! {
//!     #[derive(Clone, Debug)]
//!     pub struct Invalid {
//!         #[extends]
//!         part: Part,
//!     }
//!     impl Invalid {
//!         #[r#override(Part::missing)]
//!         fn missing(&mut self) {}
//!     }
//! }
//! ```

use std::{
    any::{Any, TypeId},
    ops::{Deref, DerefMut},
};

use super::{Instance, InstanceId};

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
    type Api: InstanceReceiver + ?Sized + 'static;
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

/// A borrowed class API that retains the complete instance's identity.
pub struct InstanceRef<'a, T: InstanceType> {
    api: &'a T::Api,
}

impl<'a, T: InstanceType> InstanceRef<'a, T> {
    /// Returns the API with the original instance borrow's lifetime.
    pub fn into_api(self) -> &'a T::Api {
        self.api
    }
    /// Returns the owning instance's stable identifier.
    pub fn id(&self) -> InstanceId {
        self.api.as_instance().id()
    }
    /// Returns the owning instance's name.
    pub fn name(&self) -> &str {
        self.api.as_instance().name()
    }
    /// Returns the owning instance's concrete class name.
    pub fn class_name(&self) -> &'static str {
        self.api.as_instance().class_name()
    }
    /// Returns the complete instance.
    pub fn instance(&self) -> &dyn Instance {
        self.api.as_instance()
    }
}

impl<T: InstanceType> Deref for InstanceRef<'_, T> {
    type Target = T::Api;
    fn deref(&self) -> &Self::Target {
        self.api
    }
}

/// A mutable class API whose methods dispatch on the complete instance.
pub struct InstanceMut<'a, T: InstanceType> {
    api: &'a mut T::Api,
}

impl<'a, T: InstanceType> InstanceMut<'a, T> {
    /// Returns the mutable API with the original instance borrow's lifetime.
    pub fn into_api(self) -> &'a mut T::Api {
        self.api
    }
    /// Returns the owning instance's stable identifier.
    pub fn id(&self) -> InstanceId {
        self.api.as_instance().id()
    }
    /// Returns the owning instance's name.
    pub fn name(&self) -> &str {
        self.api.as_instance().name()
    }
    /// Returns the owning instance's concrete class name.
    pub fn class_name(&self) -> &'static str {
        self.api.as_instance().class_name()
    }
    /// Returns the complete instance.
    pub fn instance(&self) -> &dyn Instance {
        self.api.as_instance()
    }
    /// Returns the complete mutable instance.
    pub fn instance_mut(&mut self) -> &mut dyn Instance {
        self.api.as_instance_mut()
    }
}

impl<T: InstanceType> Deref for InstanceMut<'_, T> {
    type Target = T::Api;
    fn deref(&self) -> &Self::Target {
        self.api
    }
}

impl<T: InstanceType> DerefMut for InstanceMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.api
    }
}

impl dyn Instance {
    /// Borrows an inherited API while preserving this instance's identity.
    pub fn view<T: InstanceType>(&self) -> Option<InstanceRef<'_, T>> {
        let query = self
            .query(TypeId::of::<T>())?
            .downcast_ref::<InstanceQuery<T::Api>>()?;
        Some(InstanceRef {
            api: (query.shared)(self.as_any()),
        })
    }

    /// Mutably borrows an inherited API with the owning receiver's overrides.
    pub fn view_mut<T: InstanceType>(&mut self) -> Option<InstanceMut<'_, T>> {
        let query = self
            .query(TypeId::of::<T>())?
            .downcast_ref::<InstanceQuery<T::Api>>()?;
        Some(InstanceMut {
            api: (query.mutable)(self.as_any_mut()),
        })
    }

    /// Tests generated class ancestry without changing exact downcast semantics.
    pub fn is_a<T: InstanceType>(&self) -> bool {
        self.query(TypeId::of::<T>()).is_some()
    }
}
