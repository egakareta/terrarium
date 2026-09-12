use std::{
    env,
    fmt::Display,
    process,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use glam::Vec3;
use terrarium::{Camera, Color3, Part, PartShape, Renderer, RendererError, Workspace};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

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
            warmup_frames: 10,
            measured_frames: 240,
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
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    workspace: Workspace,
    warmup_remaining: usize,
    samples: Vec<Sample>,
}

impl App {
    fn new(config: Config) -> Self {
        let workspace = create_benchmark_workspace(config.parts, config.width, config.height);
        Self {
            config,
            window: None,
            renderer: None,
            workspace,
            warmup_remaining: config.warmup_frames,
            samples: Vec::with_capacity(config.measured_frames),
        }
    }

    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        let started = Instant::now();
        if let Err(error) = renderer.render(&self.workspace) {
            report_renderer_error(error);
            event_loop.exit();
            return;
        }
        let cpu_submission = started.elapsed();
        if let Err(error) = renderer.wait_for_gpu() {
            report_renderer_error(error);
            event_loop.exit();
            return;
        }

        if self.warmup_remaining > 0 {
            self.warmup_remaining -= 1;
        } else {
            self.samples.push(Sample {
                cpu_submission,
                gpu_complete: started.elapsed(),
            });
            if self.samples.len() == self.config.measured_frames {
                print_report(self.config, &self.samples);
                event_loop.exit();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Terrarium render benchmark")
                        .with_inner_size(PhysicalSize::new(self.config.width, self.config.height)),
                )
                .expect("create benchmark window"),
        );
        let size = window.inner_size();
        self.workspace
            .current_camera
            .resize(size.width, size.height);

        let renderer = match pollster::block_on(Renderer::new_with_present_mode(
            window.clone(),
            wgpu::PresentMode::Immediate,
        )) {
            Ok(renderer) => renderer,
            Err(error) => {
                report_renderer_error(error);
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                self.workspace
                    .current_camera
                    .resize(size.width, size.height);
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_frame(event_loop);
                if !self.samples.is_empty() && self.samples.len() == self.config.measured_frames {
                    return;
                }
                window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn create_benchmark_workspace(part_count: usize, width: u32, height: u32) -> Workspace {
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

    for index in 0..part_count {
        let column = index % side;
        let row = index / side;
        let mut part = Part::new("");
        part.shape = match index % 5 {
            0 => PartShape::Block,
            1 => PartShape::Ball,
            2 => PartShape::Cylinder,
            3 => PartShape::Wedge,
            _ => PartShape::CornerWedge,
        };
        part.position = Vec3::new(
            (column as f32 - side as f32 * 0.5) * spacing,
            0.35 + (index % 7) as f32 * 0.06,
            (row as f32 - side as f32 * 0.5) * spacing,
        );
        part.size = Vec3::splat(0.82);
        part.orientation = Vec3::new(0.0, (index % 360) as f32, 0.0);
        part.color = Color3::new(
            0.24 + (index % 5) as f32 * 0.12,
            0.32 + (index % 3) as f32 * 0.16,
            0.40 + (index % 4) as f32 * 0.11,
        );
        workspace.add_part(part);
    }

    workspace
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
    let config = match parse_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}\nUse --help for usage.");
            process::exit(2);
        }
    };
    println!(
        "Rendering {} parts for {} warmup + {} measured frames...",
        config.parts, config.warmup_frames, config.measured_frames
    );

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop
        .run_app(&mut App::new(config))
        .expect("run benchmark event loop");
}
