use crate::egui::*;

#[derive(Clone, Debug)]
struct Finger {
    device: TouchDeviceId,
    id: TouchId,
    position: Pos2,
}

impl Finger {
    fn matches(&self, device: TouchDeviceId, id: TouchId) -> bool {
        self.device == device && self.id == id
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct TouchControls {
    pub detected: bool,
    pub movement: Vec2,
    pub look_delta: Vec2,
    joystick: Option<Finger>,
    look: Option<Finger>,
    viewport: Option<Rect>,
}

impl TouchControls {
    pub fn clear(&mut self) {
        self.movement = Vec2::ZERO;
        self.look_delta = Vec2::ZERO;
        self.joystick = None;
        self.look = None;
    }

    pub fn observe_input(&mut self, input: &InputState) {
        let viewport = input.content_rect();
        if self.viewport != Some(viewport) {
            self.clear();
            self.viewport = Some(viewport);
        }
        self.detected |= input.has_touch_screen();
    }

    pub fn process(&mut self, input: &InputState, captured: impl Fn(Pos2) -> bool) {
        let viewport = input.content_rect();
        let Some((center, radius)) = joystick_geometry(viewport) else {
            self.clear();
            return;
        };

        for event in &input.events {
            let Event::Touch {
                device_id,
                id,
                phase,
                pos,
                ..
            } = *event
            else {
                continue;
            };
            self.detected = true;
            let is_joystick = self
                .joystick
                .as_ref()
                .is_some_and(|finger| finger.matches(device_id, id));
            let is_look = self
                .look
                .as_ref()
                .is_some_and(|finger| finger.matches(device_id, id));

            match phase {
                TouchPhase::Start if viewport.contains(pos) && !captured(pos) => {
                    let finger = Finger {
                        device: device_id,
                        id,
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
                    self.look_delta += (pos - finger.position) * input.pixels_per_point;
                    finger.position = pos;
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if is_joystick {
                        self.joystick = None;
                        self.movement = Vec2::ZERO;
                    }
                    if is_look {
                        self.look = None;
                        if phase == TouchPhase::Cancel {
                            self.look_delta = Vec2::ZERO;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn move_joystick(&mut self, position: Pos2, center: Pos2, radius: f32) {
        let displacement = (position - center) / (radius * 0.65);
        let length = displacement.length();
        let dead_zone = 0.1;
        self.movement = if length > dead_zone {
            displacement / length * ((length - dead_zone) / (1.0 - dead_zone)).min(1.0)
        } else {
            Vec2::ZERO
        };
    }

    pub fn paint(&self, context: &Context) {
        let Some((center, radius)) = self.viewport.and_then(joystick_geometry) else {
            return;
        };
        let painter = context.layer_painter(LayerId::new(
            Order::Foreground,
            crate::egui::Id::new("terrarium_camera_joystick"),
        ));
        painter.circle(
            center,
            radius,
            Color32::from_black_alpha(80),
            Stroke::new(2.0, Color32::from_white_alpha(110)),
        );
        let tick = radius * 0.75;
        for direction in [
            vec2(1.0, 0.0),
            vec2(-1.0, 0.0),
            vec2(0.0, 1.0),
            vec2(0.0, -1.0),
        ] {
            painter.line_segment(
                [center + direction * tick, center + direction * (tick + 6.0)],
                Stroke::new(2.0, Color32::from_white_alpha(140)),
            );
        }
        painter.circle(
            center + self.movement * radius * 0.65,
            radius * 0.3,
            Color32::from_white_alpha(if self.joystick.is_some() { 190 } else { 100 }),
            Stroke::new(1.0, Color32::from_white_alpha(160)),
        );
    }
}

fn joystick_geometry(viewport: Rect) -> Option<(Pos2, f32)> {
    if !viewport.is_finite() || viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return None;
    }
    let radius = 64.0_f32
        .min(viewport.width() * 0.2)
        .min(viewport.height() * 0.25);
    let margin = 24.0_f32.min(radius * 0.5);
    Some((
        pos2(
            viewport.left() + margin + radius,
            viewport.bottom() - margin - radius,
        ),
        radius,
    ))
}
