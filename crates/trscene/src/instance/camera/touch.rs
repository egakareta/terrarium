use crate::{FrameInput, TouchPhase, glam::Vec2};

#[derive(Clone, Debug)]
struct Finger {
    device: u64,
    id: u64,
    position: Vec2,
}

#[derive(Clone, Debug, Default)]
pub(super) struct TouchControls {
    pub detected: bool,
    pub movement: Vec2,
    pub look_delta: Vec2,
    joystick: Option<Finger>,
    look: Option<Finger>,
    viewport: Option<[f32; 2]>,
}

impl TouchControls {
    pub fn clear(&mut self) {
        self.movement = Vec2::ZERO;
        self.look_delta = Vec2::ZERO;
        self.joystick = None;
        self.look = None;
    }

    pub fn has_fingers(&self) -> bool {
        self.joystick.is_some() || self.look.is_some()
    }

    pub fn observe_input(&mut self, input: &FrameInput) {
        if self.viewport != Some(input.size) {
            self.clear();
            self.viewport = Some(input.size);
        }
        self.detected |= input.touchscreen;
    }

    pub fn geometry(&self) -> Option<[f32; 3]> {
        let [width, height] = self.viewport?;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let radius = 64.0_f32.min(width * 0.2).min(height * 0.25);
        let margin = 24.0_f32.min(radius * 0.5);
        Some([margin + radius, height - margin - radius, radius])
    }

    pub fn process(&mut self, input: &FrameInput) {
        let [width, height] = input.size;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            self.clear();
            return;
        }
        let radius = 64.0_f32.min(width * 0.2).min(height * 0.25);
        let margin = 24.0_f32.min(radius * 0.5);
        let center = Vec2::new(margin + radius, height - margin - radius);
        for event in &input.touches {
            let pos = Vec2::from_array(event.position);
            if !pos.is_finite() {
                continue;
            }
            let matches = |finger: &Finger| finger.device == event.device && finger.id == event.id;
            let is_joystick = self.joystick.as_ref().is_some_and(matches);
            let is_look = self.look.as_ref().is_some_and(matches);
            self.detected = true;
            match event.phase {
                TouchPhase::Start
                    if pos.x >= 0.0
                        && pos.x <= width
                        && pos.y >= 0.0
                        && pos.y <= height
                        && !event.captured =>
                {
                    let finger = Finger {
                        device: event.device,
                        id: event.id,
                        position: pos,
                    };
                    if pos.distance(center) <= radius {
                        if self.joystick.is_none() {
                            self.joystick = Some(finger);
                            self.move_joystick(pos, center, radius);
                        }
                    } else if self.look.is_none() {
                        self.look = Some(finger);
                    }
                }
                TouchPhase::Move if is_joystick => self.move_joystick(pos, center, radius),
                TouchPhase::Move if is_look => {
                    let finger = self.look.as_mut().unwrap();
                    self.look_delta += (pos - finger.position) * input.scale;
                    finger.position = pos;
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if is_joystick {
                        self.joystick = None;
                        self.movement = Vec2::ZERO;
                    }
                    if is_look {
                        self.look = None;
                        if event.phase == TouchPhase::Cancel {
                            self.look_delta = Vec2::ZERO;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn move_joystick(&mut self, position: Vec2, center: Vec2, radius: f32) {
        let displacement = (position - center) / (radius * 0.65);
        let length = displacement.length();
        self.movement = if length > 0.1 {
            displacement / length * ((length - 0.1) / 0.9).min(1.0)
        } else {
            Vec2::ZERO
        };
    }
}
