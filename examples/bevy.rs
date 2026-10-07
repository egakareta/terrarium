//! Bevy systems driving ordinary Terrarium instances.

use terrarium::{
    bevy::{app, ecs::prelude::*, time::Time},
    glam::Vec3,
    *,
};

struct Orbit {
    id: InstanceId,
    radius: f32,
    height: f32,
    speed: f32,
    phase: f32,
}

#[derive(Resource, Default)]
struct Orbits(Vec<Orbit>);

struct SceneUi;

impl terrarium::App for SceneUi {
    fn ui(&mut self, _engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Window::new("Bevy").show(ui, |ui| {
            ui.label("Bevy systems animate ordinary Terrarium parts.");
        });
    }
}

fn animate_orbits(time: Res<Time>, orbits: Res<Orbits>, mut scene: NonSendMut<Workspace>) {
    let elapsed = time.elapsed_secs();
    for orbit in &orbits.0 {
        let angle = orbit.phase + elapsed * orbit.speed;
        if let Some(part) = scene.get_mut::<Part>(orbit.id) {
            part.with_position(Vec3::new(
                angle.cos() * orbit.radius,
                orbit.height + (angle * 1.7).sin() * 0.35,
                angle.sin() * orbit.radius,
            ))
            .with_orientation(Vec3::new(0.0, angle.to_degrees(), 0.0));
        }
    }
}

fn setup(mut scene: NonSendMut<Workspace>, mut orbits: ResMut<Orbits>) {
    scene.lighting.with_clock_time(17.5);
    scene.current_camera = Camera::new(
        Vec3::new(8.5, 5.5, 10.0),
        Vec3::new(0.0, 1.5, 0.0),
        16.0 / 9.0,
    )
    .with_zfar(100.0);

    scene.add_child(
        Part::new()
            .with_name("Floor")
            .with_size(Vec3::new(16.0, 0.4, 16.0))
            .with_position(Vec3::new(0.0, -0.2, 0.0))
            .with_color(Color3::new(0.12, 0.16, 0.22)),
    );

    for (index, (color, phase)) in [
        (Color3::new(0.95, 0.28, 0.22), 0.0),
        (Color3::new(0.24, 0.72, 0.95), 1.25),
        (Color3::new(0.38, 0.90, 0.48), 2.5),
        (Color3::new(0.95, 0.72, 0.22), 3.75),
        (Color3::new(0.75, 0.38, 0.95), 5.0),
    ]
    .into_iter()
    .enumerate()
    {
        let id = scene.add_child(
            Part::new()
                .with_name(format!("Orb{index}"))
                .with_shape(if index % 2 == 0 {
                    PartShape::Ball
                } else {
                    PartShape::Block
                })
                .with_size(Vec3::splat(0.8))
                .with_position(Vec3::new(0.0, 1.5, 0.0))
                .with_color(color)
                .with_can_collide(false),
        );

        orbits.0.push(Orbit {
            id,
            radius: 3.0,
            height: 1.5,
            speed: 0.8,
            phase,
        });
    }
}

fn main() {
    let mut app = app::App::new();
    app.init_resource::<Orbits>()
        .add_systems(app::Startup, setup)
        .add_systems(app::Update, animate_orbits);

    Terrarium::preset(terrarium::WorkspacePreset::Empty)
        .with_local_player(None)
        .with_bevy(app)
        .run(|_engine| Ok::<_, AppCreationError>(SceneUi))
        .unwrap();
}
