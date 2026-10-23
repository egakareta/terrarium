use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Write},
};

pub use trdevtools_derive::*;

use crate::{glam::*, *};

/// A value understood by the property editor.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// A checkbox value.
    Bool(bool),
    /// A finite scalar value.
    Number(f32),
    /// Editable text.
    Text(String),
    /// JSON text validated before assignment.
    Json(String),
    /// Components of a vector or transform.
    Vector(Vec<f32>),
    /// Linear RGB color channels.
    Color([f32; 3]),
    /// An enum variant and its possible values.
    Choice {
        /// Current variant name.
        selected: String,
        /// Available variant names.
        options: Vec<String>,
    },
}

/// An instance field exposed to the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// Dot-separated field path, used when applying an edit.
    pub path: String,
    /// Current field value.
    pub value: PropertyValue,
    /// Whether the inspector can edit the field.
    pub editable: bool,
}

struct DebugPreview(String);

impl Write for DebugPreview {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let remaining = 256usize.saturating_sub(self.0.len());
        if text.len() <= remaining {
            self.0.push_str(text);
            return Ok(());
        }
        let mut end = remaining;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&text[..end]);
        self.0.push('…');
        Err(fmt::Error)
    }
}

impl Property {
    /// Creates an editable property.
    pub fn new(path: impl Into<String>, value: PropertyValue) -> Self {
        Self {
            path: path.into(),
            value,
            editable: true,
        }
    }

    /// Creates a field displayed using its debug representation.
    pub fn readonly(path: impl Into<String>, value: &impl Debug) -> Self {
        let mut preview = DebugPreview(String::new());
        let _ = write!(&mut preview, "{value:?}");
        Self {
            path: path.into(),
            value: PropertyValue::Text(preview.0),
            editable: false,
        }
    }

    /// Adds a containing field name to this property's path.
    pub fn prefixed(mut self, prefix: &str) -> Self {
        self.path = if self.path.is_empty() {
            prefix.to_owned()
        } else {
            format!("{prefix}.{}", self.path)
        };
        self
    }
}

/// An error applying a property edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PropertyError {
    /// The path does not identify an editable field.
    #[error("property is unavailable or read-only")]
    NotFound,
    /// The value has the wrong type or cannot be represented by the field.
    #[error("invalid property value")]
    InvalidValue,
}

/// Reflection used by the inspector to enumerate and edit fields.
pub trait Inspect {
    /// Returns this value's fields in declaration order.
    fn properties(&self) -> Vec<Property>;
    /// Assigns an editable field, returning an error without changing it on failure.
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError>;

    /// Checks an edit against the field's type and allowed values without changing it.
    fn validate_property(&self, path: &str, value: &PropertyValue) -> Result<(), PropertyError> {
        let property = self
            .properties()
            .into_iter()
            .find(|property| property.path == path && property.editable)
            .ok_or(PropertyError::NotFound)?;
        let valid = match (&property.value, value) {
            (PropertyValue::Bool(_), PropertyValue::Bool(_))
            | (PropertyValue::Text(_), PropertyValue::Text(_)) => true,
            (PropertyValue::Number(_), PropertyValue::Number(value)) => value.is_finite(),
            (PropertyValue::Vector(current), PropertyValue::Vector(value)) => {
                current.len() == value.len() && value.iter().all(|value| value.is_finite())
            }
            (PropertyValue::Color(_), PropertyValue::Color(value)) => {
                value.iter().all(|value| value.is_finite())
            }
            (PropertyValue::Choice { options, .. }, PropertyValue::Choice { selected, .. }) => {
                options.contains(selected)
            }
            (PropertyValue::Json(_), PropertyValue::Json(value)) => {
                serde_json::from_str::<BTreeMap<String, serde_json::Value>>(value).is_ok()
            }
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(PropertyError::InvalidValue)
        }
    }
}

/// Returns the relative path of a property contained in `field`.
#[doc(hidden)]
pub fn property_tail<'a>(path: &'a str, field: &str) -> Option<&'a str> {
    if path == field {
        Some("")
    } else {
        path.strip_prefix(field)?.strip_prefix('.')
    }
}

macro_rules! scalar {
    ($ty:ty, $variant:ident, $validate:expr) => {
        impl Inspect for $ty {
            fn properties(&self) -> Vec<Property> {
                vec![Property::new("", PropertyValue::$variant(self.to_owned()))]
            }
            fn set_property(
                &mut self,
                path: &str,
                value: PropertyValue,
            ) -> Result<(), PropertyError> {
                if !path.is_empty() {
                    return Err(PropertyError::NotFound);
                }
                match value {
                    PropertyValue::$variant(value) if ($validate)(&value) => {
                        *self = value;
                        Ok(())
                    }
                    _ => Err(PropertyError::InvalidValue),
                }
            }
        }
    };
}

scalar!(bool, Bool, |_: &bool| true);
scalar!(f32, Number, |value: &f32| value.is_finite());
scalar!(String, Text, |_: &String| true);

impl<T: Inspect + ?Sized> Inspect for Box<T> {
    fn properties(&self) -> Vec<Property> {
        (**self).properties()
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        (**self).set_property(path, value)
    }
    fn validate_property(&self, path: &str, value: &PropertyValue) -> Result<(), PropertyError> {
        (**self).validate_property(path, value)
    }
}

impl Inspect for BTreeMap<String, serde_json::Value> {
    fn properties(&self) -> Vec<Property> {
        vec![Property::new(
            "",
            PropertyValue::Json(serde_json::to_string(self).unwrap()),
        )]
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        if !path.is_empty() {
            return Err(PropertyError::NotFound);
        }
        let PropertyValue::Json(json) = value else {
            return Err(PropertyError::InvalidValue);
        };
        *self = serde_json::from_str(&json).map_err(|_| PropertyError::InvalidValue)?;
        Ok(())
    }
}

impl Inspect for Color3 {
    fn properties(&self) -> Vec<Property> {
        vec![Property::new(
            "",
            PropertyValue::Color([self.r, self.g, self.b]),
        )]
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        if !path.is_empty() {
            return Err(PropertyError::NotFound);
        }
        let PropertyValue::Color(value) = value else {
            return Err(PropertyError::InvalidValue);
        };
        if !value.iter().all(|value| value.is_finite()) {
            return Err(PropertyError::InvalidValue);
        }
        *self = Color3::new(value[0], value[1], value[2]);
        Ok(())
    }
}

impl<const N: usize> Inspect for [f32; N] {
    fn properties(&self) -> Vec<Property> {
        vec![Property::new("", PropertyValue::Vector(self.to_vec()))]
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        if !path.is_empty() {
            return Err(PropertyError::NotFound);
        }
        let PropertyValue::Vector(values) = value else {
            return Err(PropertyError::InvalidValue);
        };
        if values.len() != N || !values.iter().all(|value| value.is_finite()) {
            return Err(PropertyError::InvalidValue);
        }
        self.copy_from_slice(&values);
        Ok(())
    }
}

impl Inspect for Vec3 {
    fn properties(&self) -> Vec<Property> {
        self.to_array().properties()
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        let mut values = self.to_array();
        values.set_property(path, value)?;
        *self = Self::from_array(values);
        Ok(())
    }
}

impl Inspect for Mat4 {
    fn validate_property(&self, path: &str, value: &PropertyValue) -> Result<(), PropertyError> {
        let mut copy = *self;
        copy.set_property(path, value.clone())
    }
    fn properties(&self) -> Vec<Property> {
        let (scale, rotation, position) = self.to_scale_rotation_translation();
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
        let orientation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
        [
            ("position", position),
            ("orientation", orientation),
            ("size", scale),
        ]
        .into_iter()
        .flat_map(|(name, value)| {
            value
                .properties()
                .into_iter()
                .map(move |property| property.prefixed(name))
        })
        .collect()
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        if !matches!(path, "position" | "orientation" | "size") {
            return Err(PropertyError::NotFound);
        }
        let (mut scale, mut rotation, mut position) = self.to_scale_rotation_translation();
        match path {
            "position" => position.set_property("", value)?,
            "size" => scale.set_property("", value)?,
            "orientation" => {
                let mut angles = Vec3::ZERO;
                angles.set_property("", value)?;
                rotation = Quat::from_euler(
                    EulerRot::XYZ,
                    angles.x.to_radians(),
                    angles.y.to_radians(),
                    angles.z.to_radians(),
                );
            }
            _ => unreachable!(),
        }
        let transform = Self::from_scale_rotation_translation(scale, rotation, position);
        if !transform.is_finite() {
            return Err(PropertyError::InvalidValue);
        }
        *self = transform;
        Ok(())
    }
}

impl Inspect for MeshMaterialSlots {
    fn properties(&self) -> Vec<Property> {
        Face::ALL
            .into_iter()
            .flat_map(|face| {
                self.get(face)
                    .copied()
                    .unwrap_or_default()
                    .properties()
                    .into_iter()
                    .map(move |property| property.prefixed(&format!("{face:?}")))
            })
            .collect()
    }
    fn set_property(&mut self, path: &str, value: PropertyValue) -> Result<(), PropertyError> {
        for face in Face::ALL {
            if let Some(tail) = property_tail(path, &format!("{face:?}")) {
                let mut material = self.get(face).copied().unwrap_or_default();
                material.set_property(tail, value)?;
                self.set(face, material);
                return Ok(());
            }
        }
        Err(PropertyError::NotFound)
    }
}
