use std::{cell::RefCell, collections::HashMap, sync::OnceLock};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder, ipc::InvokeBody};

use crate::{
    interop::{HostApp, Runtime},
    native_surface::{Bounds, Layout, Presenter, visible_regions},
    trscene::FrameInput,
    *,
};

type Initializer = Box<dyn FnOnce(&tauri::WebviewWindow) -> Result<NativeHost, AppCreationError>>;
thread_local! {
    static HOST: RefCell<Option<NativeHost>> = const { RefCell::new(None) };
    static INITIALIZER: RefCell<Option<Initializer>> = const { RefCell::new(None) };
    static START_ERROR: RefCell<Option<AppCreationError>> = const { RefCell::new(None) };
}
static DEFAULT_GPU: OnceLock<Result<RenderState, String>> = OnceLock::new();

pub(crate) fn render_state(
    mut options: WgpuConfiguration,
    customized: bool,
) -> Result<RenderState, AppCreationError> {
    let instance = wgpu::Instance::new(std::mem::replace(
        &mut options.instance_descriptor,
        wgpu::InstanceDescriptor::new_without_display_handle(),
    ));
    if customized {
        return pollster::block_on(RenderState::create(options, instance, None));
    }
    DEFAULT_GPU
        .get_or_init(|| {
            pollster::block_on(RenderState::create(options, instance, None))
                .map_err(|e| e.to_string())
        })
        .clone()
        .map_err(Into::into)
}

pub(crate) fn run_headless<A: App>(
    mut runtime: Runtime<A>,
    size: [u32; 2],
    mut remaining: Option<usize>,
) -> Result<Option<Engine>, AppCreationError> {
    while remaining != Some(0) {
        runtime.frame(FrameInput {
            size: size.map(|value| value as f32),
            ..Default::default()
        })?;
        runtime.engine().renderer().finish_frame();
        if runtime.snapshot().close {
            break;
        }
        if let Some(remaining) = remaining.as_mut() {
            *remaining -= 1;
        }
    }
    Ok(Some(Box::new(runtime).into_engine()))
}

struct NativeHost {
    runtime: Box<dyn HostApp>,
    main: Presenter,
    window: tauri::WebviewWindow,
    viewports: HashMap<String, Presenter>,
}
impl NativeHost {
    fn frame(&mut self, input: FrameInput, layout: Layout) -> Result<bool, AppCreationError> {
        let scale = if input.scale.is_finite() {
            input.scale.clamp(0.1, 8.0)
        } else {
            1.0
        };
        self.runtime.frame(input)?;
        let engine = self.runtime.engine();
        let size = engine.renderer().size();
        let state = &engine.creation_context().wgpu_render_state;
        let holes: Vec<_> = layout
            .occlusions
            .iter()
            .filter_map(|bounds| Layout::physical_rect(*bounds, scale, size))
            .collect();
        let regions = visible_regions(size, &holes);
        self.main.layout(
            state,
            Bounds {
                x: (layout.scene[0] * scale).round() as i32,
                y: (layout.scene[1] * scale).round() as i32,
                width: size[0],
                height: size[1],
            },
            self.window.scale_factor()?,
            regions,
        )?;
        self.main.present(state, |target, regions| {
            engine.renderer().paint_native(target, regions, None)
        })?;
        let mut active = Vec::new();
        for region in layout.viewports {
            let id: u64 = region.id.parse()?;
            if !region.bounds.iter().all(|value| value.is_finite()) {
                continue;
            }
            let width = (region.bounds[2] * scale).round().clamp(1.0, 8192.0) as u32;
            let height = (region.bounds[3] * scale).round().clamp(1.0, 8192.0) as u32;
            let bounds = Bounds {
                x: ((region.bounds[0] + layout.scene[0]) * scale)
                    .round()
                    .clamp(-8192.0, 8192.0) as i32,
                y: ((region.bounds[1] + layout.scene[1]) * scale)
                    .round()
                    .clamp(-8192.0, 8192.0) as i32,
                width,
                height,
            };
            let local = [
                region.clip[0] - region.bounds[0],
                region.clip[1] - region.bounds[1],
                region.clip[2],
                region.clip[3],
            ];
            let clip = Layout::physical_rect(local, scale, [width, height]);
            let holes: Vec<_> = region
                .occlusions
                .iter()
                .filter_map(|hole| {
                    Layout::physical_rect(
                        [
                            hole[0] - region.bounds[0],
                            hole[1] - region.bounds[1],
                            hole[2],
                            hole[3],
                        ],
                        scale,
                        [width, height],
                    )
                })
                .collect();
            let regions = clip
                .map(|clip| {
                    visible_regions([width, height], &holes)
                        .into_iter()
                        .filter_map(|rect| rect.intersection(clip))
                        .collect()
                })
                .unwrap_or_default();
            if let std::collections::hash_map::Entry::Vacant(entry) =
                self.viewports.entry(region.key.clone())
            {
                entry.insert(Presenter::new(&self.window, state, [width, height])?);
            }
            let presenter = self
                .viewports
                .get_mut(&region.key)
                .expect("viewport presenter was inserted");
            presenter.layout(state, bounds, self.window.scale_factor()?, regions)?;
            presenter.present(state, |target, regions| {
                engine.renderer().paint_native(target, regions, Some(id))
            })?;
            active.push(region.key);
        }
        self.viewports.retain(|id, _| active.contains(id));
        Ok(self.runtime.is_close_requested())
    }
    fn parent_destroyed(&mut self) {
        self.main.parent_destroyed();
        for presenter in self.viewports.values_mut() {
            presenter.parent_destroyed();
        }
    }
}

pub(crate) fn start<A: App, E: Into<AppCreationError>>(
    mut config: Terrarium<'_>,
    options: WgpuConfiguration,
    initialize: impl FnOnce(&mut Engine) -> Result<A, E> + 'static,
) -> Result<Option<Engine>, AppCreationError> {
    let title = config.title.to_owned();
    let size = config.size;
    let preset = config.workspace_preset;
    let ui = config.ui.to_owned();
    let bundle_fonts = config.bundle_fonts;
    let context = config.take_native_context();
    #[cfg(feature = "bevy")]
    let bevy = config.take_bevy_app();
    INITIALIZER.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |window| {
            let physical = window.inner_size()?;
            let (main, state) = Presenter::initialize(
                window,
                options,
                [physical.width.max(1), physical.height.max(1)],
            )?;
            let mut launcher = Terrarium::preset(preset)
                .with_size(size)
                .with_ui(&ui)
                .with_bundled_fonts(bundle_fonts);
            #[cfg(feature = "bevy")]
            if let Some(app) = bevy {
                launcher = launcher.with_bevy(app);
            }
            let runtime = launcher.initialize(state, initialize)?;
            Ok(NativeHost {
                runtime: Box::new(runtime),
                main,
                window: window.clone(),
                viewports: HashMap::new(),
            })
        }))
    });
    let result = tauri::Builder::default()
        .setup(move |app| {
            let result: Result<NativeHost, AppCreationError> = (|| {
                let window = match app.get_webview_window("main") {
                    Some(window) => window,
                    None => {
                        WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                            .title(&title)
                            .inner_size(size[0] as f64, size[1] as f64)
                            .build()?
                    }
                };
                let initializer = INITIALIZER
                    .with(|slot| slot.borrow_mut().take())
                    .ok_or("native initializer is unavailable")?;
                let runtime = initializer(&window)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                Ok(runtime)
            })();
            match result {
                Ok(runtime) => HOST.with(|host| *host.borrow_mut() = Some(runtime)),
                Err(error) => {
                    START_ERROR.with(|slot| *slot.borrow_mut() = Some(error));
                    app.handle().exit(1);
                }
            }
            Ok(())
        })
        .invoke_handler(|invoke| {
            let command = invoke.message.command().to_owned();
            if !matches!(
                command.as_str(),
                "terrarium_frame" | "terrarium_state" | "terrarium_command"
            ) {
                return false;
            }
            let payload = match invoke.message.payload() {
                InvokeBody::Json(value) => value.clone(),
                InvokeBody::Raw(_) => {
                    invoke.resolver.reject("expected a JSON command payload");
                    return true;
                }
            };
            let resolver = invoke.resolver;
            let rejection = resolver.clone();
            let dispatch_rejection = resolver.clone();
            let handle = invoke.message.webview().app_handle().clone();
            let close_handle = handle.clone();
            let dispatch = handle.run_on_main_thread(move || {
                let result: Result<(), AppCreationError> = HOST.with(|host| {
                    let mut host = host.borrow_mut();
                    let host = host.as_mut().ok_or("engine has stopped")?;
                    match command.as_str() {
                        "terrarium_frame" => {
                            let input = serde_json::from_value(payload["input"].clone())?;
                            let layout = serde_json::from_value(payload["layout"].clone())?;
                            let close = host.frame(input, layout)?;
                            if payload["snapshot"].as_bool() == Some(false) {
                                resolver.resolve(close);
                            } else {
                                resolver.resolve(host.runtime.snapshot());
                            }
                            if close {
                                close_handle.exit(0);
                            }
                        }
                        "terrarium_state" => resolver.resolve(host.runtime.snapshot()),
                        "terrarium_command" => {
                            let command = payload["command"]
                                .as_str()
                                .ok_or("command must be a string")?;
                            resolver.resolve(
                                host.runtime.command(command, payload["payload"].clone())?,
                            );
                        }
                        _ => unreachable!(),
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    rejection.reject(error.to_string());
                }
            });
            if let Err(error) = dispatch {
                dispatch_rejection.reject(error.to_string());
            }
            true
        })
        .build(context.unwrap_or_else(|| tauri::generate_context!()));
    let result = result.map(|app| {
        app.run_return(|_, event| {
            if matches!(
                event,
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::Destroyed,
                    ref label,
                    ..
                } if label == "main"
            ) {
                HOST.with(|host| {
                    if let Some(host) = host.borrow_mut().as_mut() {
                        host.parent_destroyed();
                    }
                });
            }
        })
    });
    INITIALIZER.with(|slot| slot.borrow_mut().take());
    let engine = HOST.with(|host| {
        host.borrow_mut()
            .take()
            .map(|host| host.runtime.into_engine())
    });
    result?;
    if let Some(error) = START_ERROR.with(|slot| slot.borrow_mut().take()) {
        return Err(error);
    }
    Ok(engine)
}
