use super::{
    Game, Screen,
    mobile_hud::{self, EditAction, Paint},
};
use horizon_client::{
    gamepad::PadState,
    hud::{HudBatch, rgba},
    mobile_input::{self, TouchPad, TouchSettings},
};
use winit::application::ApplicationHandler;
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

pub(super) struct Mobile {
    touch: TouchPad,
    pad: PadState,
    keys: PadState,
    using_pad: bool,
    menu_finger: Option<u64>,
    settings: TouchSettings,
    editing: bool,
    drag: Option<(u64, usize)>,
    device: Option<i32>,
    checked: std::time::Instant,
}
impl Default for Mobile {
    fn default() -> Self {
        let mut settings = TouchSettings::load();
        // The original layout put slot arrows over the health/hunger bars.
        // Migrate only their old defaults; preserve deliberately edited layouts.
        let original = mobile_input::layout();
        for (index, center) in [(8, [0.30, 0.93]), (9, [0.70, 0.93])] {
            if settings.centers[index] == original[index].center {
                settings.centers[index] = center;
            }
        }
        Self {
            touch: TouchPad::default(),
            pad: PadState::default(),
            keys: PadState::default(),
            using_pad: false,
            menu_finger: None,
            settings,
            editing: false,
            drag: None,
            device: None,
            checked: std::time::Instant::now(),
        }
    }
}
impl Mobile {
    pub(super) fn clear(&mut self) {
        self.touch.clear();
        self.pad = PadState::default();
        self.keys = PadState::default();
        self.device = None;
        self.menu_finger = None;
        self.drag = None;
    }
    pub(super) fn state(&self, size: Option<PhysicalSize<u32>>) -> PadState {
        let size = size.unwrap_or(PhysicalSize::new(1, 1));
        let mut touch = self.touch.state(
            [size.width as f32, size.height as f32],
            &self.settings.controls(),
        );
        touch.right = touch
            .right
            .map(|v| (v * self.settings.look_sensitivity).clamp(-1.0, 1.0));
        self.pad.merge(&self.keys).merge(&touch)
    }
}
impl Game<'_> {
    pub(super) fn mobile_poll(&mut self) {
        if self.mobile.checked.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }
        self.mobile.checked = std::time::Instant::now();
        if self
            .mobile
            .device
            .is_some_and(|id| !crate::android_controller_exists(id))
        {
            self.mobile.clear();
            self.mobile.using_pad = false;
        }
    }
    pub(super) fn mobile_device_event(&mut self, device: DeviceId, event: &DeviceEvent) -> bool {
        use winit::platform::android::DeviceIdExtAndroid;
        if !matches!(
            event,
            DeviceEvent::Motion { .. } | DeviceEvent::Button { .. }
        ) {
            return false;
        }
        let id = device.android_device_id();
        if self.mobile.device != Some(id) {
            self.mobile.pad = PadState::default();
            self.mobile.keys = PadState::default();
            self.mobile.device = Some(id);
        }
        match event {
            DeviceEvent::Motion { axis, value } => {
                mobile_input::android_axis(&mut self.mobile.pad, *axis, *value as f32)
            }
            DeviceEvent::Button { button, state } => {
                mobile_input::android_key(
                    &mut self.mobile.keys,
                    *button,
                    *state == ElementState::Pressed,
                );
            }
            _ => return false,
        }
        if self
            .mobile
            .pad
            .merge(&self.mobile.keys)
            .active(&self.settings.controller)
        {
            self.mobile.using_pad = true;
            self.mobile.touch.clear();
            self.last_input = std::time::Instant::now();
        }
        true
    }
    pub(super) fn mobile_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        if matches!(event, WindowEvent::Focused(false) | WindowEvent::Resized(_)) {
            self.mobile.clear();
        }
        let WindowEvent::Touch(touch) = event else {
            return false;
        };
        if touch.phase == TouchPhase::Started && self.mobile.using_pad {
            self.mobile.pad = PadState::default();
            self.mobile.keys = PadState::default();
        }
        self.mobile.using_pad = false;
        self.last_input = std::time::Instant::now();
        let playing = self.title.is_none() && self.screen == Screen::Playing;
        if playing {
            let size = self.window.as_ref().unwrap().inner_size();
            let point = [
                touch.location.x as f32 / size.width.max(1) as f32,
                touch.location.y as f32 / size.height.max(1) as f32,
            ];
            let down = touch.phase == TouchPhase::Started;
            let toolbar =
                mobile_hud::toolbar(size.width as f32, size.height as f32, self.mobile.editing);
            if down
                && let Some((action, _, _)) = toolbar.iter().find(|(_, rect, _)| {
                    mobile_hud::hit(*rect, [touch.location.x as f32, touch.location.y as f32])
                })
            {
                match action {
                    EditAction::Toggle => {
                        self.mobile.editing = !self.mobile.editing;
                        self.mobile.touch.clear();
                        self.mobile.drag = None;
                        if let Err(e) = self.mobile.settings.save() {
                            log::warn!("Touch settings: {e}");
                        }
                    }
                    EditAction::SizeDown => {
                        self.mobile.settings.scale = (self.mobile.settings.scale - 0.1).max(0.5)
                    }
                    EditAction::SizeUp => {
                        self.mobile.settings.scale = (self.mobile.settings.scale + 0.1).min(1.6)
                    }
                    EditAction::AlphaDown => {
                        self.mobile.settings.opacity =
                            self.mobile.settings.opacity.saturating_sub(25).max(30)
                    }
                    EditAction::AlphaUp => {
                        self.mobile.settings.opacity =
                            self.mobile.settings.opacity.saturating_add(25).min(230)
                    }
                }
                return true;
            }
            if self.mobile.editing {
                if down {
                    let controls = self.mobile.settings.controls();
                    let unit = size.width.min(size.height) as f32;
                    self.mobile.drag = controls
                        .iter()
                        .position(|c| {
                            let dx = (point[0] - c.center[0]) * size.width as f32;
                            let dy = (point[1] - c.center[1]) * size.height as f32;
                            dx.hypot(dy) <= c.radius * unit
                        })
                        .map(|index| (touch.id, index));
                } else if let Some((finger, index)) = self.mobile.drag
                    && finger == touch.id
                {
                    self.mobile.settings.centers[index] = point.map(|v| v.clamp(0.04, 0.96));
                    if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                        self.mobile.drag = None;
                    }
                }
                return true;
            }
            if touch.phase == TouchPhase::Cancelled {
                self.mobile.touch.clear();
            } else {
                self.mobile.touch.event(
                    touch.id,
                    [touch.location.x as f32, touch.location.y as f32],
                    [size.width as f32, size.height as f32],
                    touch.phase == TouchPhase::Started,
                    touch.phase == TouchPhase::Ended,
                    &self.mobile.settings.controls(),
                );
            }
        } else {
            self.mobile.touch.clear();
            if touch.phase == TouchPhase::Started && self.mobile.menu_finger.is_none() {
                self.mobile.menu_finger = Some(touch.id);
            }
            if self.mobile.menu_finger == Some(touch.id) {
                self.window_event(
                    event_loop,
                    id,
                    WindowEvent::CursorMoved {
                        device_id: touch.device_id,
                        position: touch.location,
                    },
                );
                if matches!(
                    touch.phase,
                    TouchPhase::Started | TouchPhase::Ended | TouchPhase::Cancelled
                ) {
                    self.window_event(
                        event_loop,
                        id,
                        WindowEvent::MouseInput {
                            device_id: touch.device_id,
                            state: if touch.phase == TouchPhase::Started {
                                ElementState::Pressed
                            } else {
                                ElementState::Released
                            },
                            button: MouseButton::Left,
                        },
                    );
                }
                if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    self.mobile.menu_finger = None;
                }
            }
        }
        true
    }
    pub(super) fn draw_mobile(&self, batch: &mut HudBatch) {
        if (self.mobile.using_pad && self.mobile.settings.hide_with_controller)
            || self.title.is_some()
            || self.screen != Screen::Playing
        {
            return;
        }
        let atlas = &self.gui.atlas;
        let unit = batch.width.min(batch.height);
        let controls = self.mobile.settings.controls();
        let size = self.window.as_ref().map(|w| w.inner_size());
        let framebuffer = size
            .map(|s| [s.width as f32, s.height as f32])
            .unwrap_or([batch.width, batch.height]);
        let state = self.mobile.touch.state(framebuffer, &controls);
        let mut paint = Paint { batch, atlas };
        for (index, control) in controls.iter().enumerate() {
            let center = [
                control.center[0] * paint.batch.width,
                control.center[1] * paint.batch.height,
            ];
            let radius = control.radius * unit;
            let held = self.mobile.touch.position(index).is_some();
            let alpha = self.mobile.settings.opacity;
            let selected = self.mobile.editing && self.mobile.drag.is_some_and(|(_, i)| i == index);
            let pressed = held || selected;
            // Large stick hit areas stay generous, while the look area stays
            // almost invisible until used. The user can still drag it in edit mode.
            if index < 2 {
                let visible = index == 0 || held || self.mobile.editing;
                if !visible {
                    continue;
                }
                paint.disk(
                    center,
                    radius * 0.90,
                    rgba(236, 237, 243, if pressed { 65 } else { 35 }),
                );
                paint.disk(center, radius * 0.88, rgba(22, 24, 31, alpha / 2));
                let offset = if index == 0 { state.left } else { state.right };
                let thumb = [
                    center[0] + offset[0] * radius * 0.52,
                    center[1] - offset[1] * radius * 0.52,
                ];
                paint.disk(
                    thumb,
                    radius * 0.34,
                    rgba(22, 24, 31, alpha.saturating_add(25)),
                );
                paint.disk(
                    thumb,
                    radius * 0.29,
                    if pressed {
                        rgba(154, 134, 255, 150)
                    } else {
                        rgba(236, 237, 243, 70)
                    },
                );
                if index == 0 {
                    for direction in [[0., -0.68], [0., 0.68], [-0.68, 0.], [0.68, 0.]] {
                        paint.disk(
                            [
                                center[0] + direction[0] * radius,
                                center[1] + direction[1] * radius,
                            ],
                            radius * 0.025,
                            mobile_hud::ink(false),
                        );
                    }
                }
                continue;
            }
            paint.disk(
                center,
                radius,
                if pressed {
                    rgba(154, 134, 255, 180)
                } else {
                    rgba(236, 237, 243, 45)
                },
            );
            paint.disk(
                center,
                radius - 1. / paint.batch.scale,
                rgba(22, 24, 31, alpha),
            );
            if pressed {
                paint.disk(center, radius * 0.87, rgba(154, 134, 255, 55));
            }
            paint.icon(center, radius * 0.85, index, mobile_hud::ink(pressed));
        }
        for (action, rect, label) in
            mobile_hud::toolbar(paint.batch.width, paint.batch.height, self.mobile.editing)
        {
            paint.capsule(rect, rgba(22, 24, 31, 185));
            let [x, y, w, h] = rect;
            if label.is_empty() {
                paint.icon(
                    [x + w / 2., y + h / 2.],
                    h * 0.50,
                    10,
                    mobile_hud::ink(false),
                );
            } else {
                paint.batch.text(
                    atlas,
                    label,
                    x + (w - HudBatch::text_width(atlas, label)) / 2.,
                    y + (h - 8.) / 2.,
                    if matches!(action, EditAction::Toggle) {
                        rgba(202, 189, 255, 255)
                    } else {
                        u32::MAX
                    },
                    false,
                );
            }
        }
        if self.mobile.editing {
            let hint = "Arrastra los controles para moverlos";
            let width = HudBatch::text_width(atlas, hint);
            let x = (paint.batch.width - width) / 2.;
            let y = unit * 0.225;
            paint.capsule([x - 8., y - 4., width + 16., 17.], rgba(22, 24, 31, 200));
            paint.batch.text(atlas, hint, x, y, u32::MAX, false);
        }
    }
}

/// Android's system Back pauses the game or returns through the existing menus.
pub(super) fn translate_event(mut event: WindowEvent) -> WindowEvent {
    use winit::keyboard::{Key, KeyCode, NamedKey, NativeKeyCode, PhysicalKey};
    if let WindowEvent::KeyboardInput { event: key, .. } = &mut event
        && key.physical_key == PhysicalKey::Unidentified(NativeKeyCode::Android(4))
    {
        key.physical_key = PhysicalKey::Code(KeyCode::Escape);
        key.logical_key = Key::Named(NamedKey::Escape);
    }
    event
}
