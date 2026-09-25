#![cfg(all(feature = "javascript", not(target_arch = "wasm32")))]

use terrarium::{AppCreationError, Instance, Part, Terrarium};

#[test]
fn configured_script_directory_starts_with_relative_imports() -> Result<(), AppCreationError> {
    Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(1))
        .with_scripts(terrarium::scripts!("tests/fixtures/scripts"))
        .run(|engine| {
            let names = engine
                .workspace
                .get_all::<Part>()
                .map(Instance::name)
                .collect::<Vec<_>>();
            assert_eq!(names, ["embedded helper"]);
            Ok::<(), AppCreationError>(())
        })?;
    Ok(())
}
