//! Headless rendering: `cargo bench --bench render_benchmark --features vulkan`.
//!
//! Defaults to 20,000 parts at 1280x720, 60 warmup frames, and 360 measured frames.
//! Configure the scene with `TERRARIUM_BENCH_PARTS`, `TERRARIUM_BENCH_WIDTH`,
//! `TERRARIUM_BENCH_HEIGHT`, and `TERRARIUM_BENCH_WARMUP`.
//! Pass `-- --iters N`, `-- --time SECONDS`, or the framework's baseline flags.

#[cfg(not(target_family = "wasm"))]
#[path = "render_benchmark/workload.rs"]
mod workload;

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::{
        env,
        fmt::Display,
        str::FromStr,
        sync::{OnceLock, mpsc},
        time::{Duration, Instant},
    };

    use anyhow::{Context, Result, ensure};
    use terrarium::{trrender::Renderer, wgpu};
    use wgpu_benchmark::{BenchmarkContext, LoopControl, SubBenchResult};

    use super::workload::RenderWorkload;

    struct Config {
        parts: usize,
        size: [u32; 2],
        warmup_frames: usize,
    }

    impl Config {
        fn from_env() -> Result<Self> {
            let config = Self {
                parts: env_value("TERRARIUM_BENCH_PARTS", 20_000)?,
                size: [
                    env_value("TERRARIUM_BENCH_WIDTH", 1280)?,
                    env_value("TERRARIUM_BENCH_HEIGHT", 720)?,
                ],
                warmup_frames: env_value("TERRARIUM_BENCH_WARMUP", 60)?,
            };
            ensure!(config.parts > 0, "TERRARIUM_BENCH_PARTS must be positive");
            ensure!(
                config.size.iter().all(|&dimension| dimension > 0),
                "TERRARIUM_BENCH_WIDTH and TERRARIUM_BENCH_HEIGHT must be positive"
            );
            Ok(config)
        }
    }

    fn env_value<T: FromStr>(name: &str, default: T) -> Result<T>
    where
        T::Err: Display,
    {
        match env::var(name) {
            Err(env::VarError::NotPresent) => Ok(default),
            value => value
                .with_context(|| format!("cannot read {name}"))?
                .parse()
                .map_err(|error| anyhow::anyhow!("invalid {name}: {error}")),
        }
    }

    struct Sample {
        gpu: Duration,
        cpu_submission: Duration,
        frame_complete: Duration,
    }

    struct Timestamps {
        queries: wgpu::QuerySet,
        resolve: wgpu::Buffer,
        readback: wgpu::Buffer,
    }

    impl Timestamps {
        fn new(device: &wgpu::Device) -> Self {
            let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("benchmark timestamps"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            });
            let resolve = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("benchmark timestamp resolve"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("benchmark timestamp readback"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Self {
                queries,
                resolve,
                readback,
            }
        }

        fn measure(
            &self,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            renderer: &mut Renderer,
            workload: &mut RenderWorkload,
            size: [u32; 2],
        ) -> Result<Sample> {
            let started = Instant::now();
            workload.advance();
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("benchmark frame"),
            });
            encoder.write_timestamp(&self.queries, 0);
            {
                let scene = workload.workspace().render_scene();
                renderer.encode(&scene, size, &mut encoder)?;
            }
            encoder.write_timestamp(&self.queries, 1);
            encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
            encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, 16);
            let submission = queue.submit(Some(encoder.finish()));
            let cpu_submission = started.elapsed();

            let (sender, receiver) = mpsc::channel();
            self.readback
                .map_async(wgpu::MapMode::Read, .., move |result| {
                    let _ = sender.send(result);
                });
            device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(60)),
            })?;
            receiver.recv()??;
            let mapped = self.readback.get_mapped_range(..)?;
            let start = u64::from_ne_bytes(mapped[..8].try_into()?);
            let end = u64::from_ne_bytes(mapped[8..16].try_into()?);
            drop(mapped);
            self.readback.unmap();

            let ticks = end
                .checked_sub(start)
                .filter(|&ticks| ticks > 0)
                .ok_or_else(|| anyhow::anyhow!("GPU timestamps were zero or wrapped"))?;
            Ok(Sample {
                gpu: Duration::from_secs_f64(
                    ticks as f64 * f64::from(queue.get_timestamp_period()) / 1_000_000_000.0,
                ),
                cpu_submission,
                frame_complete: started.elapsed(),
            })
        }
    }

    pub fn run(mut ctx: BenchmarkContext) -> Result<Vec<SubBenchResult>> {
        let config = Config::from_env()?;
        let size = config.size;
        // Renderer caches retain device resources, so their native instance must stay alive too.
        static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();
        let instance = INSTANCE.get_or_init(|| {
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env())
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))?;
        let info = adapter.get_info();
        println!(
            "Adapter: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
        if info.device_type == wgpu::DeviceType::Cpu {
            println!("Software adapter: these timings do not represent hardware GPU performance.");
        }
        let required_features =
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
        let supports_timestamps = adapter.features().contains(required_features);
        if ctx.is_test() && !supports_timestamps {
            println!(
                "Skipping GPU timing in test mode: encoder timestamp queries are unsupported."
            );
            return Ok(Vec::new());
        }
        ensure!(
            supports_timestamps,
            "adapter does not support timestamp queries inside command encoders"
        );
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("GPU benchmark"),
                required_features,
                ..Default::default()
            }))?;
        ensure!(
            size.iter()
                .all(|&dimension| dimension <= device.limits().max_texture_dimension_2d),
            "requested dimensions exceed the device's texture limit"
        );
        let mut renderer =
            Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, size)?;
        renderer.with_clear_color(wgpu::Color {
            r: 0.012,
            g: 0.019,
            b: 0.050,
            a: 1.0,
        });
        let mut workload = RenderWorkload::new(config.parts, size)?;
        let timestamps = Timestamps::new(&device);
        let warmup_frames = if ctx.is_test() {
            config.warmup_frames.min(1)
        } else {
            config.warmup_frames
        };
        let scenario = format!(
            "{} parts at {}x{}, {warmup_frames} warmup",
            config.parts, size[0], size[1]
        );
        println!(
            "{scenario}; six materials, rotating animation pool, camera zoom, and block churn"
        );
        println!(
            "GPU time excludes CPU work, uploads, and readback. Completed-frame time includes readback."
        );
        for _ in 0..warmup_frames {
            workload.advance();
            renderer.prepare(&workload.workspace().render_scene(), size)?;
            renderer.wait_for_gpu()?;
        }
        ctx.default_iterations = LoopControl::Iterations(360);
        let mut cpu_total = Duration::ZERO;
        let mut frame_total = Duration::ZERO;
        let gpu = wgpu_benchmark::iter(
            &ctx,
            &format!("{scenario} / GPU render"),
            "frames",
            1,
            || {
                let sample = timestamps
                    .measure(&device, &queue, &mut renderer, &mut workload, size)
                    .expect("render benchmark measurement failed");
                cpu_total += sample.cpu_submission;
                frame_total += sample.frame_complete;
                sample.gpu
            },
        );
        let iterations = gpu.iterations;
        let report = |metric: &str, total: Duration| SubBenchResult {
            name: format!("{scenario} / {metric}"),
            avg_duration_per_iteration: total / iterations,
            iterations,
            throughput_unit: "frames".into(),
            throughput_count_per_iteration: 1,
        };
        Ok(vec![
            gpu,
            report("CPU submission", cpu_total),
            report("Frame complete", frame_total),
        ])
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    wgpu_benchmark::main(vec![wgpu_benchmark::Benchmark {
        name: "render_benchmark",
        // The framework logs callback errors without failing the process.
        func: |ctx| Ok(native::run(ctx).expect("render benchmark failed")),
    }]);
}

#[cfg(target_family = "wasm")]
fn main() {}
