use tr::{Inspect, PropertyError, PropertyValue};

#[derive(Debug, PartialEq, tr::Inspect)]
enum Mode {
    First,
    Second,
}

#[derive(tr::Inspect)]
struct Settings {
    name: String,
    mode: Mode,
}

#[test]
fn renamed_dependency_supports_struct_and_enum_inspection() {
    let mut settings = Settings {
        name: "original".into(),
        mode: Mode::First,
    };
    assert!(settings.properties().iter().any(|property| {
        property.path == "name" && property.value == PropertyValue::Text("original".into())
    }));
    settings
        .set_property("name", PropertyValue::Text("edited".into()))
        .unwrap();
    let second = PropertyValue::Choice {
        selected: "Second".into(),
        options: vec![],
    };
    settings.validate_property("mode", &second).unwrap();
    settings.set_property("mode", second).unwrap();
    assert_eq!(settings.name, "edited");
    assert_eq!(settings.mode, Mode::Second);
    assert_eq!(
        settings.set_property(
            "mode",
            PropertyValue::Choice {
                selected: "missing".into(),
                options: vec![],
            },
        ),
        Err(PropertyError::InvalidValue)
    );
    assert_eq!(settings.mode, Mode::Second);
}
