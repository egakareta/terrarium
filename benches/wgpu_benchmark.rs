//! GPU render timing: `cargo bench --bench wgpu_benchmark --features vulkan`.
//! Pass `-- --iters 10` for a short run or `-- --save-baseline NAME` to save a comparison.

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::{sync::mpsc, time::Duration};

    use anyhow::{Result, ensure};
    use terrarium::{glam::*, trrender::Renderer, wgpu, *};
    use wgpu_benchmark::{BenchmarkContext, SubBenchResult};

    const PARTS: usize = 4096;
    const SIZE: [u32; 2] = [1280, 720];

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
            scene: &RenderScene<'_>,
            size: [u32; 2],
        ) -> Result<Duration> {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("benchmark frame"),
            });
            encoder.write_timestamp(&self.queries, 0);
            renderer.encode(scene, size, &mut encoder)?;
            encoder.write_timestamp(&self.queries, 1);
            encoder.resolve_query_set(&self.queries, 0..2, &self.resolve, 0);
            encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.readback, 0, 16);
            let submission = queue.submit(Some(encoder.finish()));

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
            Ok(Duration::from_secs_f64(
                ticks as f64 * f64::from(queue.get_timestamp_period()) / 1_000_000_000.0,
            ))
        }
    }

    fn create_workspace() -> Workspace {
        let mut workspace = Workspace::new();
        workspace.lighting.clear_skybox();
        let side = (PARTS as f32).sqrt().ceil() as usize;
        let extent = side as f32 * 1.5;
        *workspace.camera_mut() = Camera::new(
            Vec3::new(extent * 0.65, extent * 0.8, extent),
            Vec3::ZERO,
            SIZE[0] as f32 / SIZE[1] as f32,
        );
        workspace.camera_mut().with_zfar(extent * 4.0 + 10.0);
        for index in 0..PARTS {
            let height = 0.5 + (index * 17 % 13) as f32 * 0.15;
            workspace.add_child(
                Part::new()
                    .with_position(Vec3::new(
                        (index % side) as f32 * 1.5 - (extent - 1.5) * 0.5,
                        height * 0.5,
                        (index / side) as f32 * 1.5 - (extent - 1.5) * 0.5,
                    ))
                    .with_size(Vec3::new(1.0, height, 1.0))
                    .with_color(Color3::new(
                        0.2 + (index % 7) as f32 * 0.1,
                        0.3 + (index % 5) as f32 * 0.1,
                        0.6,
                    )),
            );
        }
        workspace
    }

    pub fn run(ctx: BenchmarkContext) -> Result<Vec<SubBenchResult>> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
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
        ensure!(
            adapter.features().contains(required_features),
            "adapter does not support timestamp queries inside command encoders"
        );
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("GPU benchmark"),
                required_features,
                ..Default::default()
            }))?;
        let mut renderer =
            Renderer::new(&device, &queue, wgpu::TextureFormat::Rgba8UnormSrgb, SIZE)?;
        let workspace = create_workspace();
        let scene = workspace.render_scene();
        let timestamps = Timestamps::new(&device);
        println!(
            "{PARTS} parts, {}x{}, GPU render passes including shadows",
            SIZE[0], SIZE[1]
        );
        println!("CPU work, uploads, presentation, and timestamp readback are excluded.");
        let warmup_frames = if ctx.is_test() { 1 } else { 30 };
        for _ in 0..warmup_frames {
            renderer.prepare(&scene, SIZE)?;
            renderer.wait_for_gpu()?;
        }
        let result = wgpu_benchmark::iter(&ctx, "4096 parts at 1280x720", "frames", 1, || {
            timestamps
                .measure(&device, &queue, &mut renderer, &scene, SIZE)
                .expect("GPU timestamp measurement failed")
        });
        Ok(vec![result])
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {
    wgpu_benchmark::main(vec![wgpu_benchmark::Benchmark {
        name: "terrarium_gpu_render",
        func: native::run,
    }]);
}

#[cfg(target_family = "wasm")]
fn main() {}
