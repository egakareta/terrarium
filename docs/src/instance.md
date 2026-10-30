# Instance composition

The `instance!` macro builds instance APIs from named fields and inherent
methods. It can extend an existing instance, combine independent capabilities,
and expose stored or computed properties to the devtools inspector.

```rust,no_run
use terrarium::*;

instance!({
    /// A primitive with a spring.
    #[derive(Clone, Debug)]
    pub struct SpringPart {
        #[extends]
        part: Part,
        /// Spring stiffness in newtons per meter.
        #[property(default = 20.0)]
        stiffness: f32,
    }
});

fn main() {
    let spring_part = SpringPart::default().with_stiffness(30.0);
}
```

This definition inherits `Part`'s instance identity and APIs. It also generates
the `stiffness`, `set_stiffness`, and `with_stiffness` methods. The sections
below describe the field roles and generation rules in detail.

## Composition fields

`instance!` requires a concrete struct with named fields. Generic instance
types and conditional composition fields are not supported. Annotate fields
with one composition role:

| Attribute    | Role                                                                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------- |
| `#[extends]` | Inherits an instance identity and the composed type's APIs. An instance can have at most one `extends` field. |
| `#[compose]` | Adds a capability without adding another instance identity. A type can compose multiple capabilities.         |
| `#[data]`    | Provides `InstanceData` for a new instance type that does not extend another instance.                        |

An instance has at most one identity provider: either an `#[extends]` field or
an `#[data]` field. A type with only `#[compose]` fields can define a
capability without an instance identity.

```rust,no_run
use terrarium::*;

instance!({
    #[derive(Clone, Debug)]
    pub struct Motor {
        #[property(default = 2.0)]
        speed: f32,
    }
});

instance!({
    #[derive(Clone, Debug)]
    pub struct MotorPart {
        #[extends]
        part: Part,
        #[compose]
        motor: Motor,
    }
});

instance!({
    #[derive(Clone, Debug)]
    pub struct Group {
        #[data]
        data: InstanceData,
        #[property(default = String::new(), borrow, into)]
        label: String,
    }
});

fn main() {
    let motor = Motor::default().with_speed(0.0);
    let motor_part = MotorPart::default()
        .with_transparency(0.5)
        .with_speed(0.0);
    let group = Group::default().with_label("My Group");
}
```

`Motor` is a capability, while `MotorPart` has the identity and instance APIs
of `Part` as well as the APIs provided by `Motor`. `Group` declares its own
identity storage with `InstanceData`.

## Field properties and defaults

Mark a stored field with `#[property]` to generate a getter named after the
field, a `set_<field>` setter, and a consuming `with_<field>` builder. To select
accessors explicitly, list `getter`, `setter`, and/or `builder`; when any are
listed, unlisted accessors are omitted. Each flag also accepts `= true` or
`= false`. With no accessor flags, all three are generated for compatibility.
The property attribute accepts these options:

| Option                  | Effect                                                                                                         |
| ----------------------- | -------------------------------------------------------------------------------------------------------------- |
| `default = expression`  | Uses the expression to initialize the field. Without an explicit initializer, fields use `Default::default()`. |
|                         |
| `getter`                | Generates the getter. With `borrow`, also generates `<field>_mut`.                                             |
| `setter`                | Generates `set_<field>`; `setter = method` routes it through that field method.                                |
| `builder`               | Generates the consuming `with_<field>` builder.                                                                |
| `sanitize = expression` | Calls the expression as a function on values passed to the setter.                                             |
| `borrow`                | Returns `&T` from the getter; with `setter` enabled, also generates `<field>_mut(&mut self) -> &mut T`.        |
| `into`                  | Makes the builder accept `impl Into<T>` and convert its argument. The setter still accepts `T`.                |

For example, `#[property(getter)]` or
`#[property(getter = true, setter = false, builder = false)]` generates only
the getter. A getter without a setter is shown as read-only in the devtools
inspector.

For example:

```rust,ignore
#[property(default = String::new(), borrow, into)]
label: String,

#[property(default = 0.0, sanitize = sanitize_transparency)]
transparency: f32,
```

## Overrides

Replace a method inherited from a composed API by annotating a method in the
type's inherent `impl` with `#[r#override(Type::method)]`. The replacement must
keep the inherited method's name and use a borrowed receiver with concrete
arguments. Its signature must match the inherited method.

```rust,no_run
use terrarium::*;

instance!({
    #[derive(Clone, Debug)]
    pub struct PlanarPart {
        #[extends]
        part: Part,
    }

    impl PlanarPart {
        #[r#override(PVInstance::set_pivot)]
        pub fn set_pivot(&mut self, mut pivot: glam::Mat4) {
            pivot.w_axis.y = 0.0;
            self.pv_instance_mut().set_pivot(pivot);
        }
    }
});

fn main() {
    let planar_part = PlanarPart::default()
        .with_transparency(0.5)
        .set_pivot(glam::Mat4::IDENTITY);
}
```

The override is dispatched through the inherited API, including when that API
is accessed through a composed view. Further composed types inherit the
override unless they replace the same method themselves.

## Inspector properties

With `devtools` enabled, the macro generates inspector support unless
`#[instance(inspect = false)]` is set. Instance identity fields such as the
name and attributes are included along with annotated properties from the type
and its composed APIs.

An inherent getter can also be marked `#[property]` to expose a computed
property. The inspector applies edits through the matching `set_<getter>`
method. Add options to control how it is presented or validated:

| Option              | Effect                                                                                               |
| ------------------- | ---------------------------------------------------------------------------------------------------- |
| `flatten`           | Exposes the property's nested inspector fields without prefixing their paths with the getter name.   |
| `validate = method` | Calls the named method with the proposed property value. The edit is rejected if it returns `false`. |

```rust,no_run
use terrarium::*;

instance!({
    #[derive(Clone, Debug)]
    pub struct TransformData {
        #[data]
        data: InstanceData,
        #[default(glam::Mat4::IDENTITY)]
        pivot: glam::Mat4,
    }

    impl TransformData {
        #[property(flatten, validate = accepts_pivot)]
        pub fn transform(&self) -> glam::Mat4 {
            self.pivot
        }

        pub fn set_transform(&mut self, transform: glam::Mat4) {
            self.pivot = transform;
        }

        pub fn accepts_pivot(&self, pivot: glam::Mat4) -> bool {
            pivot.is_finite()
        }
    }
});

fn main() {
    let accepts_pivot = TransformData::default().accepts_pivot(glam::Mat4::IDENTITY);
}
```

Without `flatten`, nested property paths are prefixed by the getter name. For
example, a nested field under `transform` is edited at `transform.position`. With
`flatten`, its path is `position`. Use `inspect = false` when providing a
separate inspector implementation.

## Nested definitions

Definitions can be nested in Rust modules and can extend other custom
definitions. When a type is declared in a module, set
`#[instance(path = crate::module)]` to that crate-rooted module path. The
default is `crate`.

```rust,no_run
use terrarium::*;

mod models {
    use super::*;

    instance!({
        #[instance(path = crate::models)]
        #[derive(Clone, Debug)]
        pub struct SpringPart {
            #[extends]
            part: Part,
            #[property(default = 20.0)]
            stiffness: f32,
        }
    });
}

instance!({
    #[derive(Clone, Debug)]
    pub struct WeightedSpring {
        #[extends]
        spring: models::SpringPart,
        #[property(default = 2.0)]
        mass: f32,
    }
});

fn main() {
    let spring = WeightedSpring::default().with_mass(1.0);
}
```
