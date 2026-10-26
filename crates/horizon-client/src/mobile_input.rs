//! Shared mobile layout, multitouch ownership and Android controller mapping.
use crate::gamepad::{Button, PadState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub center: [f32; 2],
    pub radius: f32,
    pub button: Option<Button>,
    pub label: &'static str,
}

/// Coordinates in framebuffer fractions. Radius uses the shorter side.
pub fn layout() -> [Control; 10] {
    use Button::*;
    [
        Control {
            center: [0.13, 0.73],
            radius: 0.15,
            button: None,
            label: "MOVE",
        },
        Control {
            center: [0.72, 0.73],
            radius: 0.15,
            button: None,
            label: "LOOK",
        },
        Control {
            center: [0.92, 0.73],
            radius: 0.065,
            button: Some(South),
            label: "JUMP",
        },
        Control {
            center: [0.92, 0.49],
            radius: 0.065,
            button: Some(RightTrigger),
            label: "MINE",
        },
        Control {
            center: [0.80, 0.43],
            radius: 0.065,
            button: Some(LeftTrigger),
            label: "USE",
        },
        Control {
            center: [0.28, 0.78],
            radius: 0.06,
            button: Some(RightStick),
            label: "SNEAK",
        },
        Control {
            center: [0.08, 0.13],
            radius: 0.055,
            button: Some(Start),
            label: "PAUSE",
        },
        Control {
            center: [0.92, 0.13],
            radius: 0.055,
            button: Some(North),
            label: "BAG",
        },
        Control {
            center: [0.39, 0.86],
            radius: 0.045,
            button: Some(LeftBumper),
            label: "<",
        },
        Control {
            center: [0.61, 0.86],
            radius: 0.045,
            button: Some(RightBumper),
            label: ">",
        },
    ]
}

pub const SETTINGS_FILE: &str = "horizon-touch.json";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TouchSettings {
    pub centers: [[f32; 2]; 10],
    pub scale: f32,
    pub opacity: u8,
    pub look_sensitivity: f32,
    pub hide_with_controller: bool,
}
impl Default for TouchSettings {
    fn default() -> Self {
        Self {
            centers: layout().map(|c| c.center),
            scale: 1.0,
            opacity: 110,
            look_sensitivity: 1.0,
            hide_with_controller: true,
        }
    }
}
impl TouchSettings {
    pub fn load() -> Self {
        let mut settings: Self = std::fs::read(SETTINGS_FILE)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let defaults = Self::default();
        for (index, center) in settings.centers.iter_mut().enumerate() {
            if center.iter().any(|v| !v.is_finite()) {
                *center = defaults.centers[index];
            }
            *center = center.map(|v| v.clamp(0.04, 0.96));
        }
        if !settings.scale.is_finite() {
            settings.scale = 1.0;
        }
        settings.scale = settings.scale.clamp(0.5, 1.6);
        if !settings.look_sensitivity.is_finite() {
            settings.look_sensitivity = 1.0;
        }
        settings.look_sensitivity = settings.look_sensitivity.clamp(0.1, 3.0);
        settings
    }
    pub fn save(&self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(format!("{SETTINGS_FILE}.tmp"), bytes)?;
        std::fs::rename(format!("{SETTINGS_FILE}.tmp"), SETTINGS_FILE)
    }
    pub fn controls(&self) -> [Control; 10] {
        let mut controls = layout();
        for (index, control) in controls.iter_mut().enumerate() {
            control.center = self.centers[index];
            control.radius *= self.scale;
        }
        controls
    }
}

#[derive(Default)]
pub struct TouchPad {
    fingers: BTreeMap<u64, (usize, [f32; 2])>,
}
impl TouchPad {
    pub fn clear(&mut self) {
        self.fingers.clear();
    }
    /// The current owner position, in framebuffer pixels, for HUD feedback.
    pub fn position(&self, control: usize) -> Option<[f32; 2]> {
        self.fingers
            .values()
            .find_map(|(index, point)| (*index == control).then_some(*point))
    }
    pub fn event(
        &mut self,
        id: u64,
        position: [f32; 2],
        size: [f32; 2],
        start: bool,
        end: bool,
        controls: &[Control],
    ) {
        if end {
            self.fingers.remove(&id);
            return;
        }
        if start {
            let unit = size[0].min(size[1]);
            if let Some(index) = controls.iter().position(|c| {
                let dx = position[0] - c.center[0] * size[0];
                let dy = position[1] - c.center[1] * size[1];
                dx * dx + dy * dy <= (c.radius * unit).powi(2)
            }) {
                // One finger owns each stick/button until lifted.
                if self.fingers.values().all(|(i, _)| *i != index) {
                    self.fingers.insert(id, (index, position));
                }
            }
        } else if let Some((_, point)) = self.fingers.get_mut(&id) {
            *point = position;
        }
    }
    pub fn state(&self, size: [f32; 2], controls: &[Control]) -> PadState {
        let mut state = PadState::default();
        for &(index, point) in self.fingers.values() {
            let control = controls[index];
            if let Some(button) = control.button {
                state.set(button, true);
                if button == Button::LeftTrigger {
                    state.triggers[0] = 1.0;
                }
                if button == Button::RightTrigger {
                    state.triggers[1] = 1.0;
                }
            } else {
                let radius = control.radius * size[0].min(size[1]).max(1.0);
                let mut stick = [
                    (point[0] - control.center[0] * size[0]) / radius,
                    (control.center[1] * size[1] - point[1]) / radius,
                ];
                let length = stick[0].hypot(stick[1]).max(1.0);
                stick = stick.map(|v| v / length);
                if index == 0 {
                    state.left = stick;
                } else {
                    state.right = stick;
                }
            }
        }
        state
    }
}

/// Android KeyEvent constants, independent of Bluetooth/USB pad brand.
pub fn android_button(code: u32) -> Option<Button> {
    use Button::*;
    Some(match code {
        96 => South,
        97 => East,
        99 => West,
        100 => North,
        102 => LeftBumper,
        103 => RightBumper,
        104 => LeftTrigger,
        105 => RightTrigger,
        106 => LeftStick,
        107 => RightStick,
        108 => Start,
        109 => Select,
        19 => DPadUp,
        20 => DPadDown,
        21 => DPadLeft,
        22 => DPadRight,
        _ => return None,
    })
}
/// Digital-only triggers must also populate the analog trigger channels used by Edges.
pub fn android_key(state: &mut PadState, code: u32, down: bool) {
    let Some(button) = android_button(code) else {
        return;
    };
    state.set(button, down);
    match button {
        Button::LeftTrigger => state.triggers[0] = if down { 1.0 } else { 0.0 },
        Button::RightTrigger => state.triggers[1] = if down { 1.0 } else { 0.0 },
        _ => {}
    }
}
pub fn android_axis(state: &mut PadState, axis: u32, value: f32) {
    let value = if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    match axis {
        0 => state.left[0] = value,
        1 => state.left[1] = -value,
        11 => state.right[0] = value,
        14 => state.right[1] = -value,
        17 => state.triggers[0] = value.max(0.0),
        18 => state.triggers[1] = value.max(0.0),
        15 => {
            state.set(Button::DPadLeft, value < -0.5);
            state.set(Button::DPadRight, value > 0.5);
        }
        16 => {
            state.set(Button::DPadUp, value < -0.5);
            state.set(Button::DPadDown, value > 0.5);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simultaneous_stick_and_jump_release_independently() {
        let mut touch = TouchPad::default();
        let size = [1000., 500.];
        touch.event(1, [130., 365.], size, true, false, &layout());
        touch.event(1, [205., 365.], size, false, false, &layout());
        touch.event(2, [920., 365.], size, true, false, &layout());
        assert_eq!(touch.state(size, &layout()).left, [1., 0.]);
        assert!(touch.state(size, &layout()).held(Button::South));
        touch.event(2, [0., 0.], size, false, true, &layout());
        assert!(!touch.state(size, &layout()).held(Button::South));
        assert_eq!(touch.state(size, &layout()).left, [1., 0.]);
        touch.clear();
        assert_eq!(touch.state(size, &layout()), PadState::default());
    }
    #[test]
    fn digital_triggers_and_dpad_survive_unrelated_axis_updates() {
        let mut keys = PadState::default();
        let mut axes = PadState::default();
        android_key(&mut keys, 105, true);
        android_key(&mut keys, 19, true);
        android_axis(&mut axes, 16, 0.0);
        android_axis(&mut axes, 18, 0.0);
        let merged = keys.merge(&axes);
        assert_eq!(merged.triggers[1], 1.0);
        assert!(merged.held(Button::DPadUp));
        android_key(&mut keys, 105, false);
        assert_eq!(keys.merge(&axes).triggers[1], 0.0);
    }
    #[test]
    fn controller_axes_are_finite_and_match_pad_conventions() {
        let mut state = PadState::default();
        android_axis(&mut state, 1, -1.);
        android_axis(&mut state, 16, -1.);
        assert_eq!(state.left[1], 1.);
        assert!(state.held(Button::DPadUp));
        android_axis(&mut state, 16, 0.);
        assert!(!state.held(Button::DPadUp));
        android_axis(&mut state, 0, f32::NAN);
        assert_eq!(state.left[0], 0.);
        assert_eq!(android_button(96), Some(Button::South));
    }
}
