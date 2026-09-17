use std::{
    env,
    fmt::Display,
    process,
    str::FromStr,
    time::{Duration, Instant},
};

use terrarium::{
    Camera, Color3, Framework, Instance, InstanceId, Part, PartShape, RendererError, Workspace,
    eframe, egui,
    glam::{EulerRot, Mat4, Quat, Vec3},
};

const ANIMATED_PARTS_RATIO: usize = 4;
const ANIMATION_POOL_CHANGE_INTERVAL: usize = 30;
const CAMERA_DISTANCE_MIN_SCALE: f32 = 0.05;
const CAMERA_DISTANCE_MAX_SCALE: f32 = 0.90;
const CAMERA_ZOOM_SPEED: f32 = 0.45;

#[derive(Clone, Copy)]
struct Config {
    parts: usize,
    warmup_frames: usize,
    measured_frames: usize,
    width: u32,
    height: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            parts: 20_000,
            warmup_frames: 60,
            measured_frames: 360,
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    cpu_submission: Duration,
    gpu_complete: Duration,
}

struct App {
    config: Config,
    framework: Framework,
    workspace: Workspace,
    base_camera_pivot: Mat4,
    part_ids: Vec<InstanceId>,
    base_parts: Vec<Part>,
    animation_frame: usize,
    animation_pool_start: usize,
    warmup_remaining: usize,
    samples: Vec<Sample>,
    frame_started: Option<Instant>,
    cpu_submission: Option<Duration>,
    finished: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, config: Config) -> Result<Self, RendererError> {
        let (workspace, part_ids) =
            create_benchmark_workspace(config.parts, config.width, config.height);
        let base_camera_pivot = workspace.current_camera.pivot();
        let base_parts = workspace.get_all::<Part>().cloned().collect();
        let mut framework = Framework::new(cc, [config.width, config.height])?;
        framework.set_clear_color([0.012, 0.019, 0.050, 1.0]);
        Ok(Self {
            config,
            framework,
            workspace,
            base_camera_pivot,
            part_ids,
            base_parts,
            animation_frame: 0,
            animation_pool_start: 0,
            warmup_remaining: config.warmup_frames,
            samples: Vec::with_capacity(config.measured_frames),
            frame_started: None,
            cpu_submission: None,
            finished: false,
        })
    }

    fn finish_previous_frame(&mut self, ctx: &egui::Context) {
        let Some(started) = self.frame_started.take() else {
            return;
        };
        if let Err(error) = self.framework.renderer().wait_for_gpu() {
            report_renderer_error(error);
            self.finished = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if self.warmup_remaining > 0 {
            self.warmup_remaining -= 1;
        } else {
            self.samples.push(Sample {
                cpu_submission: self
                    .cpu_submission
                    .take()
                    .expect("scene submission time should be recorded"),
                gpu_complete: started.elapsed(),
            });
            if self.samples.len() == self.config.measured_frames {
                print_report(self.config, &self.samples);
                self.finished = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn animate_parts(&mut self) {
        let part_count = self.part_ids.len();
        let animated_count = part_count / ANIMATED_PARTS_RATIO
            + usize::from(!part_count.is_multiple_of(ANIMATED_PARTS_RATIO));

        if self.animation_frame != 0
            && self
                .animation_frame
                .is_multiple_of(ANIMATION_POOL_CHANGE_INTERVAL)
        {
            for offset in 0..animated_count {
                let index = (self.animation_pool_start + offset) % part_count;
                self.reset_part(index);
            }
            self.animation_pool_start = (self.animation_pool_start + animated_count) % part_count;
        }

        let time = self.animation_frame as f32 * 0.02;
        let camera_scale = (CAMERA_DISTANCE_MIN_SCALE + CAMERA_DISTANCE_MAX_SCALE) * 0.5
            + (CAMERA_DISTANCE_MAX_SCALE - CAMERA_DISTANCE_MIN_SCALE)
                * 0.5
                * (time * CAMERA_ZOOM_SPEED).sin();
        let (_, camera_rotation, camera_position) =
            self.base_camera_pivot.to_scale_rotation_translation();
        self.workspace
            .current_camera
            .pivot_to(Mat4::from_rotation_translation(
                camera_rotation,
                camera_position * camera_scale,
            ));

        for offset in 0..animated_count {
            let index = (self.animation_pool_start + offset) % part_count;
            let id = self.part_ids[index];
            let base = &self.base_parts[index];
            let motion = time * 0.8 + index as f32 * 0.013;
            let pulse = (time * 1.4 + index as f32 * 0.021).sin() * 0.18;
            let color_shift = time * 1.7 + index as f32 * 0.017;

            if let Some(part) = self.workspace.get_mut::<Part>(id) {
                let (_, base_rotation, base_position) =
                    base.pivot().to_scale_rotation_translation();
                let position = base_position
                    + Vec3::new(
                        motion.sin() * 0.32,
                        (motion * 1.7).cos() * 0.16,
                        motion.cos() * 0.32,
                    );
                let rotation = base_rotation
                    * Quat::from_euler(
                        EulerRot::XYZ,
                        (motion.sin() * 12.0).to_radians(),
                        (motion.cos() * 18.0).to_radians(),
                        ((motion * 0.7).sin() * 10.0).to_radians(),
                    );
                part.pivot_to(Mat4::from_rotation_translation(rotation, position));
                part.size =
                    base.size * Vec3::new(1.0 + pulse, 1.0 + pulse * 0.6, 1.0 - pulse * 0.35);
                part.color = Color3::new(
                    (base.color.r + color_shift.sin() * 0.18).clamp(0.0, 1.0),
                    (base.color.g + (color_shift + 2.1).sin() * 0.18).clamp(0.0, 1.0),
                    (base.color.b + (color_shift + 4.2).sin() * 0.18).clamp(0.0, 1.0),
                );
            }
        }
        self.animation_frame += 1;
    }

    fn reset_part(&mut self, index: usize) {
        let id = self.part_ids[index];
        let base = &self.base_parts[index];
        if let Some(part) = self.workspace.get_mut::<Part>(id) {
            part.pivot_to(base.pivot());
            part.size = base.size;
            part.color = base.color;
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.framework.clear_color()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.finished {
            return;
        }
        self.finish_previous_frame(ctx);
        if self.finished {
            return;
        }
        self.animate_parts();
        ctx.request_repaint();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.finished {
            return;
        }
        let started = Instant::now();
        if let Err(error) = self.framework.prepare(ui, &mut self.workspace) {
            report_renderer_error(error);
            self.finished = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.cpu_submission = Some(started.elapsed());
        self.frame_started = Some(started);
        self.framework.paint(ui);
    }
}

fn create_benchmark_workspace(
    part_count: usize,
    width: u32,
    height: u32,
) -> (Workspace, Vec<InstanceId>) {
    let side = (part_count as f64).sqrt().ceil() as usize;
    let spacing = 1.2;
    let extent = side as f32 * spacing;
    let camera_distance = extent * 1.25 + 5.0;
    let mut workspace = Workspace::new();
    workspace.current_camera = Camera::new(
        Vec3::new(0.0, camera_distance * 0.72, camera_distance),
        Vec3::ZERO,
        width as f32 / height.max(1) as f32,
    );
    workspace.current_camera.zfar = camera_distance * 4.0 + extent;
    let mut part_ids = Vec::with_capacity(part_count);

    for index in 0..part_count {
        let column = index % side;
        let row = index / side;
        let mut part = Part::unnamed();
        part.shape = match index % 5 {
            0 => PartShape::Block,
            1 => PartShape::Ball,
            2 => PartShape::Cylinder,
            3 => PartShape::Wedge,
            _ => PartShape::CornerWedge,
        };
        part.pivot_to(Mat4::from_rotation_translation(
            Quat::from_rotation_y(((index % 360) as f32).to_radians()),
            Vec3::new(
                (column as f32 - side as f32 * 0.5) * spacing,
                0.35 + (index % 7) as f32 * 0.06,
                (row as f32 - side as f32 * 0.5) * spacing,
            ),
        ));
        part.size = Vec3::splat(0.82);
        part.color = Color3::new(
            0.24 + (index % 5) as f32 * 0.12,
            0.32 + (index % 3) as f32 * 0.16,
            0.40 + (index % 4) as f32 * 0.11,
        );
        part_ids.push(part.set_parent(&mut workspace));
    }

    let mut floor = Part::unnamed();
    floor.pivot_to(Mat4::from_translation(Vec3::new(0.0, -0.05, 0.0)));
    floor.size = Vec3::new(extent + spacing * 2.0, 0.1, extent + spacing * 2.0);
    floor.color = Color3::new(0.08, 0.10, 0.14);
    floor.set_parent(&mut workspace);

    (workspace, part_ids, extent)
}

fn print_report(config: Config, samples: &[Sample]) {
    let mut cpu_submission = samples
        .iter()
        .map(|sample| sample.cpu_submission.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    let mut gpu_complete = samples
        .iter()
        .map(|sample| sample.gpu_complete.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    cpu_submission.sort_unstable_by(f64::total_cmp);
    gpu_complete.sort_unstable_by(f64::total_cmp);

    let gpu_average = average(&gpu_complete);
    println!("render benchmark");
    println!("  parts: {}", config.parts);
    println!(
        "  animated parts: {} (25%), pool changes every {} frames",
        config.parts / ANIMATED_PARTS_RATIO
            + usize::from(!config.parts.is_multiple_of(ANIMATED_PARTS_RATIO)),
        ANIMATION_POOL_CHANGE_INTERVAL
    );
    println!("  warmup frames: {}", config.warmup_frames);
    println!("  measured frames: {}", samples.len());
    println!("  CPU submission: {}", format_statistics(&cpu_submission));
    println!(
        "  CPU + GPU completion: {}",
        format_statistics(&gpu_complete)
    );
    println!("  completed-frame FPS: {:.1}", 1_000.0 / gpu_average);
}

fn format_statistics(samples: &[f64]) -> String {
    format!(
        "avg {:.3} ms | p50 {:.3} ms | p95 {:.3} ms | p99 {:.3} ms | min {:.3} ms | max {:.3} ms",
        average(samples),
        percentile(samples, 0.50),
        percentile(samples, 0.95),
        percentile(samples, 0.99),
        samples[0],
        samples[samples.len() - 1],
    )
}

fn average(samples: &[f64]) -> f64 {
    samples.iter().sum::<f64>() / samples.len() as f64
}

fn percentile(samples: &[f64], percentile: f64) -> f64 {
    let index = ((samples.len() - 1) as f64 * percentile).round() as usize;
    samples[index]
}

fn report_renderer_error(error: RendererError) {
    eprintln!("render benchmark failed: {error}");
}

fn parse_value<T>(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: Display,
{
    let value = args
        .next()
        .ok_or_else(|| format!("missing value for {flag}"))?;
    value
        .parse()
        .map_err(|error| format!("invalid value for {flag}: {error}"))
}

fn parse_config() -> Result<Config, String> {
    let mut config = Config::default();
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--parts" => config.parts = parse_value(&mut args, "--parts")?,
            "--warmup" => config.warmup_frames = parse_value(&mut args, "--warmup")?,
            "--frames" => config.measured_frames = parse_value(&mut args, "--frames")?,
            "--width" => config.width = parse_value(&mut args, "--width")?,
            "--height" => config.height = parse_value(&mut args, "--height")?,
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }

    if config.parts == 0 {
        return Err("--parts must be greater than zero".into());
    }
    if config.measured_frames == 0 {
        return Err("--frames must be greater than zero".into());
    }
    if config.width == 0 || config.height == 0 {
        return Err("--width and --height must be greater than zero".into());
    }
    Ok(config)
}

fn main() {
    env_logger::init();
    let config = parse_config().unwrap();
    println!(
        "Rendering {} parts for {} warmup + {} measured frames...",
        config.parts, config.warmup_frames, config.measured_frames
    );

    Framework::run_native(
        "Terrarium render benchmark",
        [config.width, config.height],
        Box::new(move |cc| Ok(Box::new(App::new(cc, config)?))),
    )
    .expect("run benchmark eframe application");
}
