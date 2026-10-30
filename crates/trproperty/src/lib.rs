#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// Generates field getters, setters, and builders in an inherent `impl`.
///
/// The macro is declarative and has no runtime or procedural-macro dependencies.
/// Method names are specified explicitly because `macro_rules!` does not
/// concatenate identifiers.
#[macro_export]
macro_rules! property_api {
    (
        impl $target:ty {
            $(
                property $field:ident : $field_type:ty {
                    $(
                        getter $(#[$getter_attribute:meta])* $getter:ident $getter_mode:ident;
                    )?
                    $(
                        setter $(#[$setter_attribute:meta])* $setter:ident;
                    )?
                    $(
                        builder $(#[$builder_attribute:meta])* $builder:ident (
                            $($argument:ident $( : $argument_type:ty)? )?
                        ) $builder_mode:ident $(=> $value:expr)?;
                    )*
                    $(
                        custom_builder $(#[$custom_builder_attribute:meta])* $custom_builder:ident (
                            $custom_argument:ident : $custom_argument_type:ty
                        ) $custom_builder_mode:ident |$custom_receiver:ident| { $($body:tt)* }
                    )*
                }
            )*
        }
    ) => {
        impl $target {
            $(
                $(
                    $crate::__property_api_getter! {
                        [$(#[$getter_attribute])*]
                        [$field] $field_type; $getter $getter_mode
                    }
                )?
                $(
                    $crate::__property_api_setter! {
                        [$(#[$setter_attribute])*]
                        [$field] $field_type; $setter
                    }
                )?
                $(
                    $crate::__property_api_builder! {
                        [$(#[$builder_attribute])*]
                        [$field] $field_type;
                        $builder(
                            $($argument $( : $argument_type)? )?
                        ) $builder_mode $(=> $value)?
                    }
                )*
                $(
                    $crate::__property_api_custom_builder! {
                        [$(#[$custom_builder_attribute])*]
                        $field $custom_builder($custom_argument: $custom_argument_type)
                        $custom_builder_mode |$custom_receiver| { $($body)* }
                    }
                )*
            )*
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_doc {
    (@getter $field:ident {$($method:tt)*}) => {
        #[doc = concat!("Returns the `", stringify!($field), "` property value.")]
        $($method)*
    };
    (@setter $field:ident {$($method:tt)*}) => {
        #[doc = concat!("Sets the `", stringify!($field), "` property value.")]
        $($method)*
    };
    (@builder $field:ident {$($method:tt)*}) => {
        #[doc = concat!("Returns this value with the `", stringify!($field), "` property updated.")]
        $($method)*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_getter {
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident copy} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                pub fn $name(&self) -> $field_type { self.$field }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident ref} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                pub fn $name(&self) -> &$field_type { &self.$field }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident ref_mut} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                pub fn $name(&mut self) -> &mut $field_type { &mut self.$field }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_setter {
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident} => {
        $crate::__property_api_doc! {
            @setter $field {
                $($attribute)*
                pub fn $name(&mut self, value: $field_type) { self.$field = value; }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_builder {
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident($argument:ident) owned} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(mut self, $argument: $field_type) -> Self {
                    self.$field = $argument;
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident($argument:ident) mut_ref} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(&mut self, $argument: $field_type) -> &mut Self {
                    self.$field = $argument;
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident($argument:ident : $argument_type:ty) owned => $value:expr} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(mut self, $argument: $argument_type) -> Self {
                    self.$field = $value;
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident($argument:ident : $argument_type:ty) mut_ref => $value:expr} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(&mut self, $argument: $argument_type) -> &mut Self {
                    self.$field = $value;
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] [$field:ident] $field_type:ty; $name:ident() owned => $value:expr} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(mut self) -> Self {
                    self.$field = $value;
                    self
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_custom_builder {
    {[$($attribute:tt)*] $field:ident $name:ident($argument:ident: $argument_type:ty) owned |$receiver:ident| { $($body:tt)* }} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(mut self, $argument: $argument_type) -> Self {
                    let $receiver = &mut self;
                    $($body)*
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] $field:ident $name:ident($argument:ident: $argument_type:ty) mut_ref |$receiver:ident| { $($body:tt)* }} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                pub fn $name(&mut self, $argument: $argument_type) -> &mut Self {
                    let $receiver = &mut *self;
                    $($body)*
                    self
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_instance_getter {
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) clone} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                fn $field(&self) -> $field_type {
                    ::core::clone::Clone::clone(&$($runtime)*::Project::<$class>::project(self).$field)
                }
            }
        }
    };
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) borrow} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                fn $field(&self) -> &$field_type {
                    &$($runtime)*::Project::<$class>::project(self).$field
                }
            }
        }
    };
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) borrow_mut($getter:ident)} => {
        $crate::__property_api_doc! {
            @getter $field {
                $($attribute)*
                fn $getter(&mut self) -> &mut $field_type {
                    &mut $($runtime)*::Project::<$class>::project_mut(self).$field
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_instance_setter {
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) setter($setter:ident) direct} => {
        $crate::__property_api_doc! {
            @setter $field {
                $($attribute)*
                fn $setter(&mut self, value: $field_type) {
                    $($runtime)*::Project::<$class>::project_mut(self).$field = value;
                }
            }
        }
    };
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) setter($setter:ident) sanitize($sanitize:expr)} => {
        $crate::__property_api_doc! {
            @setter $field {
                $($attribute)*
                fn $setter(&mut self, value: $field_type) {
                    $($runtime)*::Project::<$class>::project_mut(self).$field = ($sanitize)(value);
                }
            }
        }
    };
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) setter($setter:ident) hook($hook:ident)} => {
        $crate::__property_api_doc! {
            @setter $field {
                $($attribute)*
                fn $setter(&mut self, value: $field_type) {
                    $($runtime)*::Project::<$class>::project_mut(self).$hook(value);
                }
            }
        }
    };
    {[$($attribute:tt)*] runtime[$($runtime:tt)*] class($class:ident) field($field:ident) type($field_type:ty) setter($setter:ident) sanitize($sanitize:expr) hook($hook:ident)} => {
        $crate::__property_api_doc! {
            @setter $field {
                $($attribute)*
                fn $setter(&mut self, value: $field_type) {
                    $($runtime)*::Project::<$class>::project_mut(self).$hook(($sanitize)(value));
                }
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __property_api_instance_builder {
    {[$($attribute:tt)*] field($field:ident) setter($setter:ident) builder($builder:ident) type($field_type:ty) value} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                fn $builder(mut self, value: $field_type) -> Self
                where
                    Self: Sized,
                {
                    self.$setter(value);
                    self
                }
            }
        }
    };
    {[$($attribute:tt)*] field($field:ident) setter($setter:ident) builder($builder:ident) type($field_type:ty) into} => {
        $crate::__property_api_doc! {
            @builder $field {
                $($attribute)*
                fn $builder(mut self, value: impl Into<$field_type>) -> Self
                where
                    Self: Sized,
                {
                    self.$setter(value.into());
                    self
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    struct Settings {
        retries: u32,
        label: String,
        shader: Option<u32>,
    }

    crate::property_api! {
        impl Settings {
            property retries: u32 {
                getter retries copy;
                setter set_retries;
                builder with_retries(retries) owned;
            }
            property label: String {
                getter label ref;
                setter set_label;
                builder with_label(label) owned;
                custom_builder append_label(suffix: &str) owned |this| {
                    this.label.push_str(suffix);
                }
            }
            property shader: Option<u32> {
                getter shader copy;
                builder with_shader(shader: u32) owned => Some(shader);
                builder without_shader() owned => None;
            }
        }
    }

    #[test]
    fn generated_accessors_and_builders_preserve_their_contracts() {
        let mut settings = Settings {
            retries: 1,
            label: "initial".to_owned(),
            shader: None,
        };
        settings.set_retries(2);
        settings.set_label("updated".to_owned());
        assert_eq!(settings.retries(), 2);
        assert_eq!(settings.label(), "updated");

        let configured = Settings {
            retries: 0,
            label: String::new(),
            shader: None,
        }
        .with_retries(4)
        .with_label("ready".to_owned())
        .append_label("!")
        .with_shader(7);
        assert_eq!(configured.retries(), 4);
        assert_eq!(configured.label(), "ready!");
        assert_eq!(configured.shader(), Some(7));
        assert_eq!(configured.without_shader().shader(), None);
    }
}
