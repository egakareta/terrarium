#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
use std::sync::{OnceLock, mpsc};

use terrarium::{glam::*, *};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;
const SIZE: [u32; 2] = [WIDTH, HEIGHT];
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const WHITE: [u8; 4] = [255; 4];

type WorkspaceBuilder = Box<dyn FnOnce(&mut Workspace) + Send>;

struct OffscreenRenderer {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    renderer: Renderer,
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
struct RenderRequest {
    build_workspace: WorkspaceBuilder,
    pixels_tx: mpsc::Sender<Vec<[u8; 4]>>,
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
static RENDERER_THREAD: OnceLock<mpsc::Sender<RenderRequest>> = OnceLock::new();

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

macro_rules! rendering_test {
    ($name:ident) => {
        paste::paste! {
            #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
            #[test]
            fn $name() {
                pollster::block_on([<$name _test>]());
            }

            #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
            #[wasm_bindgen_test::wasm_bindgen_test]
            async fn $name() {
                [<$name _test>]().await;
            }
        }
    };
}

fn pixel(pixels: &[[u8; 4]], x: u32, y: u32) -> [u8; 4] {
    pixels[y as usize * WIDTH as usize + x as usize]
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
async fn render_workspace_pixels(
    build_workspace: impl FnOnce(&mut Workspace) + Send + 'static,
) -> Vec<[u8; 4]> {
    let (pixels_tx, pixels_rx) = mpsc::channel();
    let requests = RENDERER_THREAD.get_or_init(|| {
        let (requests_tx, requests_rx) = mpsc::channel::<RenderRequest>();
        std::thread::Builder::new()
            .name("terrarium-test-renderer".to_owned())
            .spawn(move || {
                let mut renderer = pollster::block_on(OffscreenRenderer::new());
                while let Ok(request) = requests_rx.recv() {
                    let pixels = pollster::block_on(render_workspace_frame(
                        &mut renderer,
                        request.build_workspace,
                    ));
                    let _ = request.pixels_tx.send(pixels);
                }
            })
            .expect("could not start the pixel-rendering thread");
        requests_tx
    });

    requests
        .send(RenderRequest {
            build_workspace: Box::new(build_workspace),
            pixels_tx,
        })
        .expect("the pixel-rendering thread exited before accepting a scene");
    pixels_rx
        .recv()
        .expect("the pixel-rendering thread exited before returning an image")
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
async fn render_workspace_pixels(
    build_workspace: impl FnOnce(&mut Workspace) + Send + 'static,
) -> Vec<[u8; 4]> {
    let mut renderer = OffscreenRenderer::new().await;
    render_workspace_frame(&mut renderer, Box::new(build_workspace)).await
}

async fn render_workspace_frame(
    renderer: &mut OffscreenRenderer,
    build_workspace: WorkspaceBuilder,
) -> Vec<[u8; 4]> {
    let mut workspace = Workspace::new();
    workspace.lighting.clear_skybox();
    *workspace.camera_mut() = Camera::new(
        Vec3::new(0.0, 0.0, 4.0),
        Vec3::ZERO,
        WIDTH as f32 / HEIGHT as f32,
    );
    build_workspace(&mut workspace);

    renderer.renderer.with_clear_color(wgpu::Color::WHITE);
    renderer
        .renderer
        .prepare(&workspace.render_scene(), SIZE)
        .expect("could not render the test scene");
    read_pixels(&renderer.renderer).await
}

impl OffscreenRenderer {
    async fn new() -> Self {
        let descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        #[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
        let instance = wgpu::Instance::new(descriptor);
        #[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
        let instance = wgpu::util::new_instance_with_webgpu_detection(descriptor).await;

        let adapter_options = wgpu::RequestAdapterOptions {
            force_fallback_adapter: true,
            ..Default::default()
        };
        let adapter = match instance.request_adapter(&adapter_options).await {
            Ok(adapter) => adapter,
            Err(_) => instance
                .request_adapter(&Default::default())
                .await
                .expect("an available WGPU adapter is required for rendering tests"),
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("could not create a WGPU device for rendering tests");
        let renderer = Renderer::new(&device, &queue, FORMAT, SIZE)
            .expect("could not create the Terrarium renderer");

        Self {
            _instance: instance,
            _adapter: adapter,
            renderer,
        }
    }
}

#[cfg(not(all(target_family = "wasm", not(target_os = "wasi"))))]
async fn read_pixels(renderer: &Renderer) -> Vec<[u8; 4]> {
    renderer
        .read_pixels()
        .expect("could not read the rendered pixels")
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
async fn read_pixels(renderer: &Renderer) -> Vec<[u8; 4]> {
    for _ in 0..300 {
        match renderer.read_pixels() {
            Ok(pixels) => return pixels,
            Err(RendererError::PixelReadbackPending) => wait_for_browser_task().await,
            Err(error) => panic!("could not read the rendered pixels: {error}"),
        }
    }

    panic!("the browser did not complete the render-target pixel readback");
}

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
async fn wait_for_browser_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        web_sys::window()
            .expect("rendering tests require a browser window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 16)
            .expect("could not schedule a browser task");
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("the browser task failed");
}
