use std::time::Duration;

use approx::assert_abs_diff_eq;
#[cfg(not(target_arch = "wasm32"))]
use terrarium::bevy::{
    EguiContext,
    app::{AppExit, Plugin, Startup, Update},
};
use terrarium::{
    bevy::{
        TerrariumPlugin, TerrariumSet,
        app::{App, PostUpdate},
        ecs::prelude::*,
        time::{Time, TimePlugin, TimeUpdateStrategy, Virtual},
    },
    glam::Vec3,
    *,
};

#[derive(Resource)]
struct Subject(InstanceId);

#[test]
fn workspace_tweens_follow_bevy_virtual_time() {
    let mut scene = Workspace::new();
    let part = scene.add_child(Part::new());
    scene
        .tweens_mut()
        .add_position(part, Tween::new(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 1.0));

    let mut app = App::new();
    app.add_plugins(TimePlugin);
    app.world_mut().insert_non_send(scene);
    app.add_plugins(TerrariumPlugin)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )));
    app.finish();
    app.cleanup();
    app.update();
    app.update();

    let position = || {
        app.world()
            .non_send::<Workspace>()
            .get::<Part>(part)
            .unwrap()
            .position()
    };
    assert_abs_diff_eq!(position().x, 1.0, epsilon = 1e-6);

    app.world_mut().resource_mut::<Time<Virtual>>().pause();
    app.update();
    assert_abs_diff_eq!(
        app.world()
            .non_send::<Workspace>()
            .get::<Part>(part)
            .unwrap()
            .position()
            .x,
        1.0,
        epsilon = 1e-6
    );

    let mut time = app.world_mut().resource_mut::<Time<Virtual>>();
    time.unpause();
    time.set_relative_speed(2.0);
    app.update();
    assert_abs_diff_eq!(
        app.world()
            .non_send::<Workspace>()
            .get::<Part>(part)
            .unwrap()
            .position()
            .x,
        3.0,
        epsilon = 1e-6
    );
}

#[test]
fn systems_can_edit_the_scene_after_tweens_advance() {
    fn raise_subject(subject: Res<Subject>, mut scene: NonSendMut<Workspace>) {
        let part = scene.get_mut::<Part>(subject.0).unwrap();
        let mut position = part.position();
        position.y = 2.0;
        part.with_position(position);
    }

    let mut app = App::new();
    app.add_plugins(TerrariumPlugin);
    let part = {
        let mut scene = app.world_mut().non_send_mut::<Workspace>();
        let part = scene.add_child(Part::new());
        scene
            .tweens_mut()
            .add_position(part, Tween::new(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 1.0));
        part
    };
    app.insert_resource(Subject(part))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            100,
        )))
        .add_systems(PostUpdate, raise_subject.after(TerrariumSet::Advance));
    app.finish();
    app.cleanup();
    app.update();
    app.update();

    let position = app
        .world()
        .non_send::<Workspace>()
        .get::<Part>(part)
        .unwrap()
        .position();
    assert_abs_diff_eq!(position.x, 1.0, epsilon = 1e-6);
    assert_abs_diff_eq!(position.y, 2.0, epsilon = 1e-6);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn attached_app_runs_until_exit_and_returns_the_edited_scene() -> Result<(), AppCreationError> {
    #[derive(Resource, Default)]
    struct Frames(u32);

    #[derive(Resource)]
    struct CloseWithContext(bool);

    struct SceneCallbacks {
        subject: InstanceId,
    }

    impl terrarium::App for SceneCallbacks {
        fn after_update(
            &mut self,
            engine: &mut Engine,
            _context: &egui::Context,
            _frame: &mut eframe::Frame,
        ) {
            let part = engine.get_mut::<Part>(self.subject).unwrap();
            let mut position = part.position();
            position.y = 4.0;
            part.with_position(position);
        }

        fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            ui.label("Ordinary Terrarium UI");
            let part = engine.get_mut::<Part>(self.subject).unwrap();
            let mut position = part.position();
            position.z = 8.0;
            part.with_position(position);
        }
    }

    fn setup(mut scene: NonSendMut<Workspace>) {
        scene.add_child(Part::new().with_name("Startup part"));
    }

    fn update(
        mut frames: ResMut<Frames>,
        subject: Res<Subject>,
        mut scene: NonSendMut<Workspace>,
        mut exit: MessageWriter<AppExit>,
        context: NonSend<EguiContext>,
        close_with_context: Res<CloseWithContext>,
    ) {
        frames.0 += 1;
        scene
            .get_mut::<Part>(subject.0)
            .unwrap()
            .with_position(Vec3::new(frames.0 as f32, 0.0, 0.0));
        if frames.0 == 3 {
            if close_with_context.0 {
                context
                    .get()
                    .send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                exit.write(AppExit::Success);
            }
        }
    }

    for close_with_context in [false, true] {
        let mut scene = Workspace::new();
        let subject = scene.add_child(Part::new().with_name("Existing part"));
        let mut app = App::new();
        app.world_mut().insert_non_send(scene);
        app.insert_resource(Subject(subject))
            .insert_resource(CloseWithContext(close_with_context))
            .init_resource::<Frames>()
            .add_systems(Startup, setup)
            .add_systems(Update, update);

        let engine = Terrarium::preset(WorkspacePreset::Baseplate)
            .with_bevy(app)
            .with_title("Bevy adapter test")
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(Some(10))
            .run(move |engine| {
                engine.add_child(Part::new().with_name("Initializer part"));
                engine
                    .bevy_app_mut()
                    .unwrap()
                    .insert_resource(Subject(subject));
                Ok::<_, AppCreationError>(SceneCallbacks { subject })
            })?
            .unwrap();

        let scene = &engine.workspace;
        assert!(
            scene
                .get_all::<Part>()
                .all(|part| part.name() != "Baseplate")
        );
        let part = scene.get::<Part>(subject).unwrap();
        assert_eq!(part.name(), "Existing part");
        assert_abs_diff_eq!(part.position().x, 3.0, epsilon = 1e-6);
        assert_abs_diff_eq!(part.position().y, 4.0, epsilon = 1e-6);
        assert_abs_diff_eq!(part.position().z, 8.0, epsilon = 1e-6);
        assert!(
            scene
                .get_all::<Part>()
                .any(|part| part.name() == "Startup part")
        );
        assert!(
            scene
                .get_all::<Part>()
                .any(|part| part.name() == "Initializer part")
        );
        if !close_with_context {
            assert_eq!(
                engine.bevy_app().unwrap().should_exit(),
                Some(AppExit::Success)
            );
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn plugin_finalization_can_populate_the_scene_before_startup() -> Result<(), AppCreationError> {
    struct ScenePlugin;

    impl Plugin for ScenePlugin {
        fn build(&self, _app: &mut App) {}

        fn finish(&self, app: &mut App) {
            app.world_mut()
                .non_send_mut::<Workspace>()
                .add_child(Part::new().with_name("Plugin part"));
        }

        fn cleanup(&self, app: &mut App) {
            app.world_mut()
                .non_send_mut::<Workspace>()
                .lighting
                .with_clock_time(17.5);
        }
    }

    fn exit(mut exit: MessageWriter<AppExit>) {
        exit.write(AppExit::Success);
    }

    for already_finalized in [false, true] {
        let mut app = App::new();
        app.add_plugins((TerrariumPlugin, ScenePlugin))
            .add_systems(Startup, exit);
        if already_finalized {
            app.finish();
            app.cleanup();
        }

        let engine = Terrarium::new()
            .with_bevy(app)
            .with_size([32, 32])
            .with_env_logger(false)
            .with_headless(Some(1))
            .run(|engine| {
                engine.add_child(Part::new().with_name("Initializer part"));
                Ok::<(), AppCreationError>(())
            })?
            .unwrap();
        let scene = &engine.workspace;
        assert!(
            scene
                .get_all::<Part>()
                .any(|part| part.name() == "Plugin part")
        );
        assert_abs_diff_eq!(scene.lighting.clock_time(), 17.5, epsilon = 1e-6);
        assert_eq!(
            engine.bevy_app().unwrap().should_exit(),
            Some(AppExit::Success)
        );
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn pending_plugins_prepare_the_scene_before_systems_run() -> Result<(), AppCreationError> {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Resource)]
    struct PreparedName(Mutex<&'static str>);

    struct PreparingPlugin(AtomicUsize);

    impl Plugin for PreparingPlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(PreparedName(Mutex::new("Unprepared scene")));
        }

        fn ready(&self, app: &App) -> bool {
            if self
                .0
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                    remaining.checked_sub(1)
                })
                .is_ok()
            {
                return false;
            }
            *app.world().resource::<PreparedName>().0.lock().unwrap() = "Prepared scene";
            true
        }
    }

    fn setup(
        name: Res<PreparedName>,
        mut scene: NonSendMut<Workspace>,
        mut exit: MessageWriter<AppExit>,
    ) {
        scene.add_child(Part::new().with_name(*name.0.lock().unwrap()));
        exit.write(AppExit::Success);
    }

    let mut app = App::new();
    app.add_plugins((TerrariumPlugin, PreparingPlugin(AtomicUsize::new(4))))
        .add_systems(Startup, setup);
    let engine = Terrarium::new()
        .with_bevy(app)
        .with_size([32, 32])
        .with_env_logger(false)
        .with_headless(Some(10))
        .run(|engine| {
            engine.add_child(Part::new().with_name("Initializer part"));
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();

    assert!(
        engine
            .workspace
            .get_all::<Part>()
            .any(|part| part.name() == "Prepared scene")
    );
    assert_eq!(
        engine.bevy_app().unwrap().should_exit(),
        Some(AppExit::Success)
    );
    Ok(())
}
