//! Serializable input shared by the browser and native webview hosts.

use serde::{Deserialize, Serialize};

/// Keyboard selection modifiers.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Modifiers {
    /// Control key.
    pub ctrl: bool,
    /// Platform command key.
    pub command: bool,
    /// Shift key.
    pub shift: bool,
}

impl Modifiers {
    /// Control key held.
    pub const CTRL: Self = Self {
        ctrl: true,
        command: false,
        shift: false,
    };
    /// Shift key held.
    pub const SHIFT: Self = Self {
        ctrl: false,
        command: false,
        shift: true,
    };
    /// No modifier keys.
    pub const NONE: Self = Self {
        ctrl: false,
        command: false,
        shift: false,
    };
}

/// The phase of a touch gesture.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum TouchPhase {
    /// Touch down.
    Start,
    /// Touch movement.
    Move,
    /// Touch released.
    End,
    /// Gesture cancelled by the host.
    Cancel,
}

/// One touch event in viewport-local CSS pixels.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TouchEvent {
    /// Input device identifier.
    pub device: u64,
    /// Finger identifier.
    pub id: u64,
    /// Gesture phase.
    pub phase: TouchPhase,
    /// Position relative to the scene canvas.
    pub position: [f32; 2],
    /// Whether the gesture started on a UI component.
    #[serde(default)]
    pub captured: bool,
}

/// A host-independent snapshot of input for one engine frame.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct FrameInput {
    /// Serialized egui input events from the host, in viewport-local CSS pixels.
    pub gui_events: Vec<serde_json::Value>,
    /// Seconds since the previous frame.
    pub delta: f32,
    /// Viewport dimensions in CSS pixels.
    pub size: [f32; 2],
    /// Physical pixels per CSS pixel.
    pub scale: f32,
    /// Whether the application is focused.
    pub focused: bool,
    /// Held physical keys using DOM `KeyboardEvent.code` names.
    pub keys: Vec<String>,
    /// Non-repeating key presses this frame.
    pub pressed: Vec<String>,
    /// Current keyboard modifiers.
    pub modifiers: Modifiers,
    /// Held mouse buttons using DOM button indices.
    pub buttons: Vec<u16>,
    /// Mouse movement in physical pixels.
    pub mouse_delta: [f32; 2],
    /// Whether a UI component owns mouse input.
    pub pointer_captured: bool,
    /// Whether a text editor owns keyboard input.
    pub keyboard_captured: bool,
    /// Touch events since the previous frame.
    pub touches: Vec<TouchEvent>,
    /// Whether the host detected touchscreen hardware.
    pub touchscreen: bool,
}

impl Default for FrameInput {
    fn default() -> Self {
        Self {
            gui_events: Vec::new(),
            delta: 1.0 / 60.0,
            size: [1280.0, 720.0],
            scale: 1.0,
            focused: true,
            keys: Vec::new(),
            pressed: Vec::new(),
            modifiers: Modifiers::NONE,
            buttons: Vec::new(),
            mouse_delta: [0.0; 2],
            pointer_captured: false,
            keyboard_captured: false,
            touches: Vec::new(),
            touchscreen: false,
        }
    }
}

impl FrameInput {
    /// Clamps frame time to the engine's supported simulation interval.
    pub fn delta_secs(&self) -> f32 {
        if self.delta.is_finite() {
            self.delta.clamp(0.0, 0.1)
        } else {
            0.0
        }
    }

    /// Returns a physical viewport size with finite, positive dimensions.
    pub fn physical_size(&self) -> [u32; 2] {
        let scale = if self.scale.is_finite() {
            self.scale.clamp(0.1, 8.0)
        } else {
            1.0
        };
        self.size.map(|value| {
            if value.is_finite() {
                (value * scale).round().clamp(1.0, 8192.0) as u32
            } else {
                1
            }
        })
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            ctrl: self.ctrl || rhs.ctrl,
            command: self.command || rhs.command,
            shift: self.shift || rhs.shift,
        }
    }
}
