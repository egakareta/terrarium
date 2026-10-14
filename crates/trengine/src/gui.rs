use crate::{AppCreationError, RenderState, trscene::FrameInput};

pub(crate) struct Gui {
    pub context: egui::Context,
    renderer: egui_wgpu::Renderer,
    state: RenderState,
    jobs: Vec<egui::ClippedPrimitive>,
    screen: egui_wgpu::ScreenDescriptor,
    free: Vec<egui::TextureId>,
    time: f64,
}

impl Gui {
    pub fn new(state: &RenderState) -> Self {
        let context = egui::Context::default();
        context.set_fonts(trfont::font_definitions());
        configure_egui_style(&context);
        Self {
            context,
            renderer: egui_wgpu::Renderer::new(
                &state.device,
                state.target_format,
                Default::default(),
            ),
            state: state.clone(),
            jobs: Vec::new(),
            free: Vec::new(),
            time: 0.0,
            screen: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [1, 1],
                pixels_per_point: 1.0,
            },
        }
    }
    pub fn input(&mut self, input: &FrameInput) -> Result<egui::RawInput, AppCreationError> {
        self.time += f64::from(input.delta_secs());
        let zoom = self.context.zoom_factor();
        let mut events = input
            .gui_events
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<Result<Vec<egui::Event>, _>>()?;
        for code in &input.pressed {
            if input.keyboard_captured && code != "F12" {
                continue;
            }
            let name = code
                .strip_prefix("Key")
                .or_else(|| code.strip_prefix("Digit"))
                .unwrap_or(code);
            if let Some(key) = egui::Key::from_name(name)
                && !events.iter().any(|event| matches!(event, egui::Event::Key { key: existing, pressed: true, .. } if *existing == key))
            {
                events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: input.modifiers.ctrl,
                        command: input.modifiers.ctrl || input.modifiers.command,
                        mac_cmd: input.modifiers.command,
                        shift: input.modifiers.shift,
                        ..Default::default()
                    },
                });
            }
        }
        for event in &mut events {
            match event {
                egui::Event::PointerMoved(position)
                | egui::Event::PointerButton { pos: position, .. }
                | egui::Event::Touch { pos: position, .. } => {
                    *position = egui::pos2(position.x / zoom, position.y / zoom)
                }
                _ => {}
            }
        }
        for touch in &input.touches {
            if !touch.captured {
                events.push(egui::Event::Touch {
                    device_id: egui::TouchDeviceId(touch.device),
                    id: egui::TouchId(touch.id),
                    phase: match touch.phase {
                        crate::trscene::TouchPhase::Start => egui::TouchPhase::Start,
                        crate::trscene::TouchPhase::Move => egui::TouchPhase::Move,
                        crate::trscene::TouchPhase::End => egui::TouchPhase::End,
                        crate::trscene::TouchPhase::Cancel => egui::TouchPhase::Cancel,
                    },
                    pos: egui::pos2(touch.position[0] / zoom, touch.position[1] / zoom),
                    force: None,
                });
            }
        }
        if input.keyboard_captured {
            self.context.memory_mut(|memory| {
                if let Some(id) = memory.focused() {
                    memory.surrender_focus(id);
                }
            });
        }
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(input.size[0], input.size[1]) / zoom,
            )),
            time: Some(self.time),
            predicted_dt: input.delta_secs(),
            focused: input.focused,
            events,
            ..Default::default()
        };
        raw.viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(input.scale);
        self.screen.size_in_pixels = input.physical_size();
        Ok(raw)
    }
    pub fn update(&mut self, mut output: egui::FullOutput) {
        self.screen.pixels_per_point = output.pixels_per_point;
        self.jobs = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer
                    .update_texture(&self.state.device, &self.state.queue, *id, delta);
            }
        }
        self.free.extend(output.textures_delta.free.drain());
        output.textures_delta.clear();
    }
    pub fn regions(&self, interactive: bool) -> Vec<[f32; 4]> {
        let zoom = self.context.zoom_factor();
        let margin = self.context.global_style().visuals.window_shadow.margin();
        let extent = margin
            .left
            .max(margin.right)
            .max(margin.top)
            .max(margin.bottom)
            .max(8.0);
        let layers = self
            .context
            .memory(|memory| memory.areas().visible_layer_ids());
        let mut regions: Vec<_> = layers
            .into_iter()
            .filter(|layer| !matches!(layer.order, egui::Order::Background | egui::Order::Debug))
            .filter_map(|layer| {
                let area = egui::AreaState::load(&self.context, layer.id)?;
                if interactive && !area.interactable {
                    return None;
                }
                let rect = area.rect().expand(if interactive { 8.0 } else { extent }) * zoom;
                rect.is_positive()
                    .then_some([rect.min.x, rect.min.y, rect.width(), rect.height()])
            })
            .collect();
        regions.sort_by(|left, right| {
            left[0]
                .total_cmp(&right[0])
                .then_with(|| left[1].total_cmp(&right[1]))
                .then_with(|| left[2].total_cmp(&right[2]))
                .then_with(|| left[3].total_cmp(&right[3]))
        });
        regions
    }
    pub fn paint(&mut self, target: &wgpu::TextureView, regions: &[[u32; 4]], clear: bool) {
        let jobs: Vec<_> = self
            .jobs
            .iter()
            .flat_map(|job| {
                regions.iter().filter_map(|&[x, y, w, h]| {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(x as f32, y as f32),
                        egui::vec2(w as f32, h as f32),
                    ) / self.screen.pixels_per_point;
                    let clip = job.clip_rect.intersect(rect);
                    clip.is_positive().then(|| egui::ClippedPrimitive {
                        clip_rect: clip,
                        primitive: job.primitive.clone(),
                    })
                })
            })
            .collect();
        if clear || !jobs.is_empty() {
            let mut encoder =
                self.state
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Terrarium egui overlay"),
                    });
            let callbacks = self.renderer.update_buffers(
                &self.state.device,
                &self.state.queue,
                &mut encoder,
                &jobs,
                &self.screen,
            );
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: if clear {
                                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                                } else {
                                    wgpu::LoadOp::Load
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    })
                    .forget_lifetime();
                self.renderer.render(&mut pass, &jobs, &self.screen);
            }
            self.state
                .queue
                .submit(callbacks.into_iter().chain([encoder.finish()]));
        }
        for id in self.free.drain(..) {
            self.renderer.free_texture(&id);
        }
    }
}
fn configure_egui_style(context: &egui::Context) {
    let surface = egui::Color32::from_rgb(13, 20, 30);
    let window = egui::Color32::from_rgb(19, 29, 42);
    let widget = egui::Color32::from_rgb(26, 39, 53);
    let hovered = egui::Color32::from_rgb(35, 56, 70);
    let active = egui::Color32::from_rgb(28, 75, 76);
    let accent = egui::Color32::from_rgb(77, 215, 194);
    let text = egui::Color32::from_rgb(230, 238, 245);
    let muted_text = egui::Color32::from_rgb(148, 167, 183);
    let border = egui::Color32::from_rgb(48, 69, 84);

    context.all_styles_mut(|style| {
        style.animation_time = 0.16;

        let visuals = &mut style.visuals;
        *visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(text);
        visuals.weak_text_color = Some(muted_text);
        visuals.hyperlink_color = accent;
        visuals.faint_bg_color = egui::Color32::from_rgb(18, 29, 40);
        visuals.extreme_bg_color = egui::Color32::from_rgb(8, 12, 18);
        visuals.text_edit_bg_color = Some(widget);
        visuals.code_bg_color = widget;
        visuals.panel_fill = surface;
        visuals.window_fill = window;
        visuals.window_stroke = egui::Stroke::new(1.0, border);
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        visuals.selection.bg_fill = egui::Color32::from_rgb(34, 87, 85);
        visuals.selection.stroke = egui::Stroke::new(1.0, accent);

        let widgets = &mut visuals.widgets;
        widgets.noninteractive.bg_fill = surface;
        widgets.noninteractive.weak_bg_fill = surface;
        widgets.noninteractive.fg_stroke.color = text;

        widgets.inactive.bg_fill = widget;
        widgets.inactive.weak_bg_fill = widget;
        widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
        widgets.inactive.fg_stroke.color = text;

        widgets.hovered.bg_fill = hovered;
        widgets.hovered.weak_bg_fill = hovered;
        widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.hovered.fg_stroke.color = text;

        widgets.active.bg_fill = active;
        widgets.active.weak_bg_fill = active;
        widgets.active.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.active.fg_stroke.color = text;

        widgets.open.bg_fill = active;
        widgets.open.weak_bg_fill = active;
        widgets.open.bg_stroke = egui::Stroke::new(1.0, accent);
        widgets.open.fg_stroke.color = text;

        for widget in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(4);
        }
    });
    context.set_theme(egui::ThemePreference::Dark);
}
