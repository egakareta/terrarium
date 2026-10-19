use crate::{
    ControllerActions, egui,
    glam::{Vec2, Vec3},
    winit::{
        event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
        keyboard::{KeyCode, PhysicalKey},
    },
};

/// Physical key bindings used by [`ControllerInput`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerKeyBindings {
    /// Keys that set the forward movement axis.
    pub forward: Vec<KeyCode>,
    /// Keys that set the backward movement axis.
    pub backward: Vec<KeyCode>,
    /// Keys that set the left movement axis.
    pub left: Vec<KeyCode>,
    /// Keys that set the right movement axis.
    pub right: Vec<KeyCode>,
    /// Keys that set the upward movement axis.
    pub up: Vec<KeyCode>,
    /// Keys that set the downward movement axis.
    pub down: Vec<KeyCode>,
    /// Keys that enable the sprint multiplier.
    pub sprint: Vec<KeyCode>,
}

impl Default for ControllerKeyBindings {
    fn default() -> Self {
        Self {
            forward: vec![KeyCode::KeyW],
            backward: vec![KeyCode::KeyS],
            left: vec![KeyCode::KeyA],
            right: vec![KeyCode::KeyD],
            up: vec![KeyCode::Space],
            down: vec![KeyCode::ControlLeft, KeyCode::ControlRight],
            sprint: vec![KeyCode::ShiftLeft, KeyCode::ShiftRight],
        }
    }
}

/// Device state translated into [`ControllerActions`].
///
/// Devices can be enabled separately. Feed events through the matching `process_*`
/// method, then call [`Self::take_actions`] once per update: movement remains held
/// until release, while mouse and touch look deltas are consumed by that call.
/// Touch uses a left-side movement joystick and assigns each other finger to look
/// when it touches down. UI capture suppresses input; losing focus clears active state.
#[derive(Clone, Debug)]
pub struct ControllerInput {
    /// Physical keys assigned to each movement action.
    pub key_bindings: ControllerKeyBindings,
    /// Whether a forward key is held.
    pub forward: bool,
    /// Whether a backward key is held.
    pub backward: bool,
    /// Whether a left key is held.
    pub left: bool,
    /// Whether a right key is held.
    pub right: bool,
    /// Whether an upward key is held.
    pub up: bool,
    /// Whether a downward key is held.
    pub down: bool,
    /// Whether a sprint key is held.
    pub sprint: bool,
    /// Accumulated mouse-look delta `(x, y)`, consumed by [`Self::take_actions`].
    pub mouse_look_delta: (f32, f32),
    mouse_scroll_delta: f32,
    /// Button that enables cursor-drag look for normalized egui input.
    pub mouse_drag_button: MouseButton,
    /// Whether mouse-look and wheel-zoom input is processed.
    pub mouse_enabled: bool,
    /// Whether keyboard movement input is processed.
    pub keyboard_enabled: bool,
    /// Whether touchscreen joystick and drag-to-look input are processed.
    pub touch_enabled: bool,
    mouse_dragging: bool,
    last_cursor_position: Option<(f64, f64)>,
    touch: super::touch::TouchControls,
}

impl Default for ControllerInput {
    fn default() -> Self {
        Self {
            key_bindings: ControllerKeyBindings::default(),
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
            sprint: false,
            mouse_look_delta: (0.0, 0.0),
            mouse_scroll_delta: 0.0,
            mouse_drag_button: MouseButton::Left,
            mouse_enabled: true,
            keyboard_enabled: true,
            touch_enabled: true,
            mouse_dragging: false,
            last_cursor_position: None,
            touch: super::touch::TouchControls::default(),
        }
    }
}

impl ControllerInput {
    /// Reads movement and consumes accumulated look and wheel input from enabled devices.
    pub fn take_actions(&mut self) -> ControllerActions {
        self.take_actions_and_scroll().0
    }

    pub(crate) fn take_actions_and_scroll(&mut self) -> (ControllerActions, f32) {
        if !self.mouse_enabled {
            self.mouse_look_delta = (0.0, 0.0);
            self.mouse_scroll_delta = 0.0;
        }
        if !self.keyboard_enabled {
            self.clear_keyboard_input();
        }
        if !self.touch_enabled {
            self.touch.clear();
        }
        let axis =
            |positive: bool, negative: bool| (i8::from(positive) - i8::from(negative)) as f32;
        let actions = ControllerActions {
            movement: Vec3::new(
                (axis(self.right, self.left) + self.touch.movement.x).clamp(-1.0, 1.0),
                axis(self.up, self.down),
                (axis(self.forward, self.backward) - self.touch.movement.y).clamp(-1.0, 1.0),
            ),
            look: Vec2::new(self.mouse_look_delta.0, self.mouse_look_delta.1)
                + Vec2::new(self.touch.look_delta.x, self.touch.look_delta.y),
            sprint: self.sprint,
        };
        self.mouse_look_delta = (0.0, 0.0);
        self.touch.look_delta = egui::Vec2::ZERO;
        (actions, std::mem::take(&mut self.mouse_scroll_delta))
    }

    /// Enables or disables all three devices.
    ///
    /// Disabling a device clears its held state, accumulated deltas, and active gestures.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.set_keyboard_enabled(enabled);
        self.set_mouse_enabled(enabled);
        self.set_touch_enabled(enabled);
    }

    /// Enables keyboard input, or clears held keys when disabling it.
    pub fn set_keyboard_enabled(&mut self, enabled: bool) {
        self.keyboard_enabled = enabled;
        if !enabled {
            self.clear_keyboard_input();
        }
    }

    /// Enables mouse input, or clears accumulated look, wheel, and drag state when disabling it.
    pub fn set_mouse_enabled(&mut self, enabled: bool) {
        self.mouse_enabled = enabled;
        if !enabled {
            self.mouse_look_delta = (0.0, 0.0);
            self.mouse_scroll_delta = 0.0;
            self.stop_mouse_drag();
        }
    }

    /// Enables touch input, or clears active gestures and hides the joystick when disabled.
    pub fn set_touch_enabled(&mut self, enabled: bool) {
        self.touch_enabled = enabled;
        if !enabled {
            self.touch.clear();
        }
    }

    /// Sets whether a touchscreen is present before touch events are observed.
    ///
    /// A later normalized input frame can detect touch again. Setting this to
    /// `false` clears any active gesture.
    pub fn set_touchscreen_detected(&mut self, detected: bool) {
        self.touch.detected = detected;
        if !detected {
            self.touch.clear();
        }
    }

    /// Returns whether at least one of keyboard, mouse, or touch is enabled.
    pub fn is_enabled(&self) -> bool {
        self.keyboard_enabled || self.mouse_enabled || self.touch_enabled
    }

    /// Returns this input with all devices enabled or disabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.set_enabled(enabled);
        self
    }

    /// Returns this input with keyboard input enabled or disabled.
    pub fn with_keyboard_enabled(mut self, enabled: bool) -> Self {
        self.set_keyboard_enabled(enabled);
        self
    }

    /// Returns this input with mouse input enabled or disabled.
    pub fn with_mouse_enabled(mut self, enabled: bool) -> Self {
        self.set_mouse_enabled(enabled);
        self
    }

    /// Returns this input with touch input enabled or disabled.
    pub fn with_touch_enabled(mut self, enabled: bool) -> Self {
        self.set_touch_enabled(enabled);
        self
    }

    /// Updates key and drag state from a winit window event.
    ///
    /// Handles keyboard, mouse-drag, and wheel events. Losing focus clears held
    /// keys and active gestures; touch gestures are read from egui input events.
    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return false;
                };
                self.process_key(key, event.state == ElementState::Pressed)
            }
            WindowEvent::MouseWheel { delta, .. } if self.mouse_enabled => {
                let zoom = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 100.0,
                };
                self.queue_mouse_scroll(zoom)
            }
            WindowEvent::MouseInput { state, button, .. } if *button == self.mouse_drag_button => {
                if !self.mouse_enabled {
                    return false;
                }
                self.mouse_dragging = *state == ElementState::Pressed;
                self.last_cursor_position = None;
                true
            }
            WindowEvent::CursorMoved { position, .. } if self.mouse_dragging => {
                if !self.mouse_enabled {
                    self.stop_mouse_drag();
                    return false;
                }
                if let Some((last_x, last_y)) = self.last_cursor_position {
                    self.process_mouse_motion((
                        (position.x - last_x) as f32,
                        (position.y - last_y) as f32,
                    ));
                }
                self.last_cursor_position = Some((position.x, position.y));
                true
            }
            WindowEvent::CursorLeft { .. } => {
                let was_dragging = self.mouse_dragging;
                self.stop_mouse_drag();
                was_dragging
            }
            WindowEvent::Focused(false) => {
                self.clear();
                false
            }
            _ => false,
        }
    }

    /// Reports whether a movement binding is down in the current egui input state.
    ///
    /// The result is independent of egui keyboard capture so an integration can
    /// route focus to the scene. Sprint alone does not count as movement.
    pub fn has_eframe_movement_input(&self, input: &egui::InputState) -> bool {
        self.keyboard_enabled
            && input.focused
            && [
                &self.key_bindings.forward,
                &self.key_bindings.backward,
                &self.key_bindings.left,
                &self.key_bindings.right,
                &self.key_bindings.up,
                &self.key_bindings.down,
            ]
            .into_iter()
            .any(|binding| eframe_key_down(input, binding))
    }

    /// Reads egui input, suppressing look over egui areas and wheel zoom over
    /// interactive widgets, and paints touch controls.
    ///
    /// Gestures are assigned at touch-down, permitting simultaneous joystick and
    /// look input. Touches starting on UI areas are left to egui.
    pub fn process_eframe_input(&mut self, context: &egui::Context) {
        let pointer_capture = context.egui_wants_pointer_input();
        let (has_wheel_input, pointer_position) = context.input(|input| {
            (
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::MouseWheel { .. })),
                input.pointer.hover_pos(),
            )
        });
        let wheel_capture = has_wheel_input
            && (context.egui_is_using_pointer()
                || pointer_position.is_some_and(|position| {
                    context
                        .interactive_rects_last_pass()
                        .iter()
                        .any(|rect| rect.contains(position))
                }));
        let keyboard_capture = context.egui_wants_keyboard_input();
        let mut captured_touches = context.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Start,
                        pos,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        let interactive_rects = if captured_touches.is_empty() {
            Vec::new()
        } else {
            context.interactive_rects_last_pass()
        };
        captured_touches.retain(|position| {
            interactive_rects
                .iter()
                .any(|rect| rect.contains(*position))
                || context
                    .layer_id_at(*position)
                    .is_some_and(|layer| layer.order != egui::Order::Background)
        });
        context.input(|input| {
            self.process_eframe_input_inner(
                input,
                pointer_capture,
                wheel_capture,
                keyboard_capture,
                |position| captured_touches.contains(&position),
            );
        });
        if self.touch_enabled && self.touch.detected {
            self.touch.paint(context);
        }
    }

    pub(crate) fn process_eframe_input_inner(
        &mut self,
        input: &egui::InputState,
        pointer_capture: bool,
        wheel_capture: bool,
        keyboard_capture: bool,
        touch_captured: impl Fn(egui::Pos2) -> bool,
    ) {
        self.touch.observe_input(input);
        if !input.focused {
            self.clear();
            return;
        }
        if self.touch_enabled {
            self.touch.process(input, touch_captured);
        } else {
            self.touch.clear();
        }
        if !self.keyboard_enabled || keyboard_capture {
            self.clear_keyboard_input();
        } else {
            let key_down = |binding: &[KeyCode]| eframe_key_down(input, binding);
            self.forward = key_down(&self.key_bindings.forward);
            self.backward = key_down(&self.key_bindings.backward);
            self.left = key_down(&self.key_bindings.left);
            self.right = key_down(&self.key_bindings.right);
            self.up = key_down(&self.key_bindings.up);
            self.down = key_down(&self.key_bindings.down);
            self.sprint = key_down(&self.key_bindings.sprint);
        }
        // Integrations synthesize pointer events from touch; do not apply them twice.
        let touch_pointer = input.any_touches()
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Touch { .. }));
        if !self.mouse_enabled || pointer_capture || touch_pointer {
            self.mouse_look_delta = (0.0, 0.0);
            self.stop_mouse_drag();
        } else {
            if let Some(button) = eframe_button(self.mouse_drag_button)
                && input.pointer.button_down(button)
            {
                let delta = input.pointer.delta() * input.pixels_per_point;
                self.process_mouse_motion((delta.x, delta.y));
            }
        }
        if !self.mouse_enabled || wheel_capture || touch_pointer {
            self.mouse_scroll_delta = 0.0;
        } else {
            for event in &input.events {
                if let egui::Event::MouseWheel { unit, delta, .. } = event {
                    let scale = match unit {
                        egui::MouseWheelUnit::Point => 1.0 / 40.0,
                        egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => 1.0,
                    };
                    let zoom = delta.y * scale;
                    self.queue_mouse_scroll(zoom);
                }
            }
        }
    }

    /// Sets every movement action bound to `key` and reports whether it matched.
    pub fn process_key(&mut self, key: KeyCode, pressed: bool) -> bool {
        if !self.keyboard_enabled {
            return false;
        }
        let mut used = false;
        for (bindings, held) in [
            (&self.key_bindings.forward, &mut self.forward),
            (&self.key_bindings.backward, &mut self.backward),
            (&self.key_bindings.left, &mut self.left),
            (&self.key_bindings.right, &mut self.right),
            (&self.key_bindings.up, &mut self.up),
            (&self.key_bindings.down, &mut self.down),
            (&self.key_bindings.sprint, &mut self.sprint),
        ] {
            if bindings.contains(&key) {
                *held = pressed;
                used = true;
            }
        }
        used
    }

    /// Accumulates raw mouse motion from a winit device event.
    pub fn process_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.process_mouse_motion((delta.0 as f32, delta.1 as f32));
        }
    }

    /// Adds a mouse delta to the pending look input when mouse input is enabled.
    pub fn process_mouse_motion(&mut self, delta: (f32, f32)) {
        if self.mouse_enabled {
            self.mouse_look_delta.0 += delta.0;
            self.mouse_look_delta.1 += delta.1;
        }
    }

    /// Clears held actions and pending look gestures without changing device settings.
    pub fn clear(&mut self) {
        self.clear_keyboard_input();
        self.mouse_look_delta = (0.0, 0.0);
        self.mouse_scroll_delta = 0.0;
        self.stop_mouse_drag();
        self.touch.clear();
    }

    fn queue_mouse_scroll(&mut self, zoom: f32) -> bool {
        let queued = self.mouse_scroll_delta + zoom;
        if zoom.is_finite() && queued.is_finite() {
            self.mouse_scroll_delta = queued;
            zoom != 0.0
        } else {
            false
        }
    }

    fn clear_keyboard_input(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.sprint = false;
    }

    fn stop_mouse_drag(&mut self) {
        self.mouse_dragging = false;
        self.last_cursor_position = None;
    }
}

fn eframe_key_down(input: &egui::InputState, binding: &[KeyCode]) -> bool {
    binding
        .iter()
        .copied()
        .any(|key| eframe_key(key).is_some_and(|key| input.key_down(key)))
}

fn eframe_key(key: KeyCode) -> Option<egui::Key> {
    use egui::Key;
    Some(match key {
        KeyCode::KeyA => Key::A,
        KeyCode::KeyB => Key::B,
        KeyCode::KeyC => Key::C,
        KeyCode::KeyD => Key::D,
        KeyCode::KeyE => Key::E,
        KeyCode::KeyF => Key::F,
        KeyCode::KeyG => Key::G,
        KeyCode::KeyH => Key::H,
        KeyCode::KeyI => Key::I,
        KeyCode::KeyJ => Key::J,
        KeyCode::KeyK => Key::K,
        KeyCode::KeyL => Key::L,
        KeyCode::KeyM => Key::M,
        KeyCode::KeyN => Key::N,
        KeyCode::KeyO => Key::O,
        KeyCode::KeyP => Key::P,
        KeyCode::KeyQ => Key::Q,
        KeyCode::KeyR => Key::R,
        KeyCode::KeyS => Key::S,
        KeyCode::KeyT => Key::T,
        KeyCode::KeyU => Key::U,
        KeyCode::KeyV => Key::V,
        KeyCode::KeyW => Key::W,
        KeyCode::KeyX => Key::X,
        KeyCode::KeyY => Key::Y,
        KeyCode::KeyZ => Key::Z,
        KeyCode::Digit0 | KeyCode::Numpad0 => Key::Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Key::Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Key::Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Key::Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Key::Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Key::Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Key::Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Key::Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Key::Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Key::Num9,
        KeyCode::Space => Key::Space,
        KeyCode::ControlLeft => Key::ControlLeft,
        KeyCode::ControlRight => Key::ControlRight,
        KeyCode::ShiftLeft => Key::ShiftLeft,
        KeyCode::ShiftRight => Key::ShiftRight,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        _ => return None,
    })
}

fn eframe_button(button: MouseButton) -> Option<egui::PointerButton> {
    use egui::PointerButton;
    Some(match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Extra1,
        MouseButton::Forward => PointerButton::Extra2,
        MouseButton::Other(_) => return None,
    })
}
