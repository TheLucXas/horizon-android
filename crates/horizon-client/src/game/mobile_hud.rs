//! Resolution-independent touch controls. Geometry uses the existing GUI batch;
//! feathered edges stay one physical pixel wide at every GUI scale.
use horizon_client::hud::{HudAtlas, HudBatch, HudVertex, rgba};
use std::f32::consts::TAU;

pub(super) struct Paint<'a> {
    pub batch: &'a mut HudBatch,
    pub atlas: &'a HudAtlas,
}
impl Paint<'_> {
    fn triangle(&mut self, points: [[f32; 2]; 3], colors: [u32; 3]) {
        let white = self.atlas.font.white();
        let uv = [(white.u0 + white.u1) / 2., (white.v0 + white.v1) / 2.];
        for (pos, color) in points.into_iter().zip(colors) {
            self.batch.vertices.push(HudVertex {
                pos,
                uv,
                color,
                depth: 0.,
            });
        }
    }
    pub fn disk(&mut self, center: [f32; 2], radius: f32, color: u32) {
        let fringe = 1. / self.batch.scale;
        let clear = color & 0x00ff_ffff;
        for i in 0..48 {
            let point = |angle: f32, radius: f32| {
                [
                    center[0] + angle.cos() * radius,
                    center[1] + angle.sin() * radius,
                ]
            };
            let a = TAU * i as f32 / 48.;
            let b = TAU * (i + 1) as f32 / 48.;
            let inner = [point(a, radius), point(b, radius)];
            let outer = [point(a, radius + fringe), point(b, radius + fringe)];
            self.triangle([center, inner[0], inner[1]], [color; 3]);
            self.triangle([inner[0], outer[0], outer[1]], [color, clear, clear]);
            self.triangle([inner[0], outer[1], inner[1]], [color, clear, color]);
        }
    }
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, color: u32) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy).max(0.001);
        let normal = [-dy / length * width / 2., dx / length * width / 2.];
        let p = [
            [a[0] + normal[0], a[1] + normal[1]],
            [b[0] + normal[0], b[1] + normal[1]],
            [b[0] - normal[0], b[1] - normal[1]],
            [a[0] - normal[0], a[1] - normal[1]],
        ];
        self.triangle([p[0], p[1], p[2]], [color; 3]);
        self.triangle([p[0], p[2], p[3]], [color; 3]);
        self.disk(a, width / 2., color);
        self.disk(b, width / 2., color);
    }
    pub fn icon(&mut self, center: [f32; 2], radius: f32, index: usize, color: u32) {
        let segments: &[([f32; 2], [f32; 2])] = match index {
            // Jump, pickaxe, interaction, sneak, pause, backpack, slot arrows.
            2 => &[
                ([-0.55, 0.0], [0., -0.55]),
                ([0., -0.55], [0.55, 0.0]),
                ([0., -0.55], [0., 0.60]),
            ],
            3 => &[
                ([-0.48, 0.55], [0.32, -0.35]),
                ([-0.44, -0.38], [0.12, -0.60]),
                ([0.12, -0.60], [0.60, -0.20]),
            ],
            4 => &[
                ([-0.48, -0.42], [-0.48, 0.42]),
                ([-0.48, 0.42], [0.48, 0.42]),
                ([0.48, 0.42], [0.48, -0.05]),
                ([-0.10, -0.23], [0.48, -0.23]),
                ([0.20, -0.52], [0.20, 0.05]),
            ],
            5 => &[
                ([-0.40, -0.10], [0., 0.10]),
                ([0., 0.10], [0.40, -0.10]),
                ([-0.48, 0.25], [0., 0.48]),
                ([0., 0.48], [0.48, 0.25]),
            ],
            6 => &[
                ([-0.23, -0.48], [-0.23, 0.48]),
                ([0.23, -0.48], [0.23, 0.48]),
            ],
            7 => &[
                ([-0.48, -0.25], [0.48, -0.25]),
                ([0.48, -0.25], [0.48, 0.52]),
                ([0.48, 0.52], [-0.48, 0.52]),
                ([-0.48, 0.52], [-0.48, -0.25]),
                ([-0.22, -0.25], [-0.22, -0.52]),
                ([-0.22, -0.52], [0.22, -0.52]),
                ([0.22, -0.52], [0.22, -0.25]),
                ([-0.20, 0.18], [0.20, 0.18]),
            ],
            8 => &[([0.23, -0.45], [-0.23, 0.]), ([-0.23, 0.], [0.23, 0.45])],
            9 => &[([-0.23, -0.45], [0.23, 0.]), ([0.23, 0.], [-0.23, 0.45])],
            10 => &[
                ([-0.45, 0.40], [0.35, -0.40]),
                ([-0.45, 0.40], [-0.08, 0.28]),
                ([0.08, -0.12], [0.35, 0.15]),
            ],
            _ => &[([-0.48, 0.], [-0.12, 0.36]), ([-0.12, 0.36], [0.52, -0.36])],
        };
        for &(a, b) in segments {
            let point = |p: [f32; 2]| [center[0] + p[0] * radius, center[1] + p[1] * radius];
            self.line(point(a), point(b), radius * 0.115, color);
        }
    }
    pub fn capsule(&mut self, rect: [f32; 4], color: u32) {
        let [x, y, width, height] = rect;
        let radius = height / 2.;
        let center = [x + width / 2., y + radius];
        let fringe = 1. / self.batch.scale;
        let clear = color & 0x00ff_ffff;
        let point = |index: usize, extra: f32| {
            let angle = -TAU / 4.
                + TAU * (index % 24) as f32 / 46.
                + if index >= 24 { TAU / 2. } else { 0. };
            let cx = if index < 24 {
                x + width - radius
            } else {
                x + radius
            };
            [
                cx + angle.cos() * (radius + extra),
                y + radius + angle.sin() * (radius + extra),
            ]
        };
        for i in 0..48 {
            let next = (i + 1) % 48;
            let a = point(i, 0.);
            let b = point(next, 0.);
            let outer_a = point(i, fringe);
            let outer_b = point(next, fringe);
            self.triangle([center, a, b], [color; 3]);
            self.triangle([a, outer_a, outer_b], [color, clear, clear]);
            self.triangle([a, outer_b, b], [color, clear, color]);
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum EditAction {
    Toggle,
    SizeDown,
    SizeUp,
    AlphaDown,
    AlphaUp,
}
pub(super) fn toolbar(
    width: f32,
    height: f32,
    editing: bool,
) -> Vec<(EditAction, [f32; 4], &'static str)> {
    let unit = width.min(height);
    let h = unit * 0.075;
    let w = unit * 0.18;
    let middle = width / 2.;
    let top = unit * 0.025;
    let mut buttons = vec![(
        EditAction::Toggle,
        [middle - w / 2., top, w, h],
        if editing { "LISTO" } else { "" },
    )];
    if editing {
        for (action, offset, y, label) in [
            (EditAction::SizeDown, -1.12, top, "Tam. -"),
            (EditAction::SizeUp, 1.12, top, "Tam. +"),
            (EditAction::AlphaDown, -0.56, top + h * 1.2, "Opac. -"),
            (EditAction::AlphaUp, 0.56, top + h * 1.2, "Opac. +"),
        ] {
            buttons.push((action, [middle + offset * w - w / 2., y, w, h], label));
        }
    }
    buttons
}
pub(super) fn hit(rect: [f32; 4], point: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    point[0] >= x && point[0] <= x + w && point[1] >= y && point[1] <= y + h
}
pub(super) fn ink(pressed: bool) -> u32 {
    if pressed {
        rgba(202, 189, 255, 255)
    } else {
        rgba(236, 237, 243, 220)
    }
}
