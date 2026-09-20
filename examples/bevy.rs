//! A small example of using Bevy ECS to drive a Terrarium scene.

use bevy_ecs::prelude::*;
use bevy_time::{Time, TimePlugin};
use terrarium::{egui::*, glam::Vec3, *};

#[derive(Component, Clone, Copy)]
struct TerrariumPart {
    id: InstanceId,
}

#[derive(Component, Clone, Copy)]
struct Orbit {
    radius: f32,
    height: f32,
    speed: f32,
    phase: f32,
}

#[derive(Component, Clone, Copy)]
struct EcsTransform {
    position: Vec3,
    yaw_degrees: f32,
}

fn animate_orbits(time: Res<Time>, mut query: Query<(&Orbit, &mut EcsTransform)>) {
    let elapsed = time.elapsed_secs();
    for (orbit, mut transform) in query.iter_mut() {
        let angle = orbit.phase + elapsed * orbit.speed;
        transform.position = Vec3::new(
            angle.cos() * orbit.radius,
            orbit.height + (angle * 1.7).sin() * 0.35,
            angle.sin() * orbit.radius,
        );
        transform.yaw_degrees = angle.to_degrees();
    }
}

struct BevyScene {
    bevy_app: bevy_app::App,
}

impl terrarium::App for BevyScene {
    fn after_update(
        &mut self,
        engine: &mut Engine,
        _context: &Context,
        _frame: &mut eframe::Frame,
    ) {
        self.bevy_app.update();

        let transforms = {
            let world = self.bevy_app.world_mut();
            let mut query = world.query::<(&TerrariumPart, &EcsTransform)>();
            query
                .iter(world)
                .map(|(part, transform)| (part.id, *transform))
                .collect::<Vec<_>>()
        };

        for (id, transform) in transforms {
            if let Some(part) = engine.get_mut::<Part>(id) {
                part.with_position(transform.position)
                    .with_orientation(Vec3::new(0.0, transform.yaw_degrees, 0.0));
            }
        }
    }

    fn ui(&mut self, _engine: &mut Engine, ui: &mut Ui, _frame: &mut eframe::Frame) {
        Window::new("Bevy ECS").show(ui, |ui| {
            ui.label("Bevy systems animate the Terrarium parts.");
        });
    }
}

fn initialize(engine: &mut Engine) -> Result<BevyScene, RendererError> {
    engine.lighting.with_clock_time(17.5);
    engine.current_camera = Camera::new(
        Vec3::new(8.5, 5.5, 10.0),
        Vec3::new(0.0, 1.5, 0.0),
        16.0 / 9.0,
    )
    .with_zfar(100.0);

    engine.add_child(
        Part::new()
            .with_name("Floor")
            .with_size(Vec3::new(16.0, 0.4, 16.0))
            .with_position(Vec3::new(0.0, -0.2, 0.0))
            .with_color(Color3::new(0.12, 0.16, 0.22)),
    );

    let mut bevy_app = bevy_app::App::new();
    bevy_app
        .add_plugins(TimePlugin)
        .add_systems(bevy_app::Update, animate_orbits);

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
        let id = engine.add_child(
            Part::new()
                .with_name(format!("EcsOrb{index}"))
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

        bevy_app.world_mut().spawn((
            TerrariumPart { id },
            Orbit {
                radius: 3.0,
                height: 1.5,
                speed: 0.8,
                phase,
            },
            EcsTransform {
                position: Vec3::new(0.0, 1.5, 0.0),
                yaw_degrees: 0.0,
            },
        ));
    }

    Ok(BevyScene { bevy_app })
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}
