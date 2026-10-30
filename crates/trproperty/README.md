# trproperty

`trproperty` provides a declarative macro for generating field getters, setters,
and builders without a procedural macro dependency. Generated methods receive
summary documentation automatically; optional attributes can add cfg gates or
more detail. Method names are explicit.

```rust
use trproperty::property_api;

struct Settings {
    retries: u32,
    label: String,
}

property_api! {
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
        }
    }
}
```

Getters support `copy`, `ref`, and `ref_mut`. Builders can consume the value
(`owned`) or mutate it in place (`mut_ref`). A builder may accept a different
argument type and map it to the field value:

```rust
use trproperty::property_api;

struct Settings {
    optional: Option<u32>,
}

property_api! {
    impl Settings {
        property optional: Option<u32> {
            builder with_optional(value: u32) owned => Some(value);
            builder without_optional() owned => None;
        }
    }
}
```

Use `custom_builder` when a builder has behavior beyond assigning one field;
name the mutable receiver in `|receiver|`. The macro supplies the signature,
return style, and summary documentation.
