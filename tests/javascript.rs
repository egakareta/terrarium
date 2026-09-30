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

    #[cfg(feature = "bevy")]
    {
        use terrarium::{
            HasPVInstance,
            bevy::{app, ecs::prelude::NonSendMut},
            glam::Vec3,
        };

        fn edit_script_part(mut scene: NonSendMut<terrarium::Workspace>) {
            let part = scene
                .get_all::<Part>()
                .find(|part| part.name() == "embedded helper")
                .unwrap()
                .id();
            scene
                .get_mut::<Part>(part)
                .unwrap()
                .with_position(Vec3::new(2.0, 3.0, 4.0));
        }

        let mut app = app::App::new();
        app.add_systems(app::Startup, edit_script_part);
        let engine = Terrarium::new()
            .with_bevy(app)
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(Some(1))
            .with_scripts(terrarium::scripts!("tests/fixtures/scripts"))
            .run(|engine| {
                assert!(
                    engine
                        .get_all::<Part>()
                        .any(|part| part.name() == "embedded helper")
                );
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        let scene = &engine.workspace;
        let names = scene
            .get_all::<Part>()
            .map(Instance::name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["embedded helper"]);
        let position = scene.get_all::<Part>().next().unwrap().position();
        approx::assert_abs_diff_eq!(position.x, 2.0, epsilon = 1e-6);
        approx::assert_abs_diff_eq!(position.y, 3.0, epsilon = 1e-6);
        approx::assert_abs_diff_eq!(position.z, 4.0, epsilon = 1e-6);
    }
    Ok(())
}
