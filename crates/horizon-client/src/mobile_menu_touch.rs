//! Single-finger menu gestures. Defer clicks in lists so a swipe never toggles a setting.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Start,
    Move,
    End,
    Cancel,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Direct,
    Scroll,
    ScrollAndDrag,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Move([f64; 2]),
    Press([f64; 2]),
    Release([f64; 2]),
    Scroll { position: [f64; 2], lines: f32 },
}
#[derive(Clone, Copy)]
enum Motion {
    Pending,
    Drag,
    Scroll,
}
struct Finger {
    id: u64,
    start: [f64; 2],
    last: [f64; 2],
    started: Instant,
    motion: Motion,
    hold_drag: bool,
}
#[derive(Default)]
pub struct MenuTouch {
    finger: Option<Finger>,
}
impl MenuTouch {
    pub fn clear(&mut self) {
        self.finger = None;
    }
    pub fn update(
        &mut self,
        id: u64,
        phase: Phase,
        position: [f64; 2],
        scale: f64,
        mode: Mode,
        now: Instant,
    ) -> Vec<Action> {
        let scale = scale.max(1.0);
        let scrollable = mode != Mode::Direct;
        if phase == Phase::Start {
            if self.finger.is_some() {
                return Vec::new();
            }
            self.finger = Some(Finger {
                id,
                start: position,
                last: position,
                started: now,
                hold_drag: mode == Mode::ScrollAndDrag,
                motion: if scrollable {
                    Motion::Pending
                } else {
                    Motion::Drag
                },
            });
            return if scrollable {
                vec![Action::Move(position)]
            } else {
                vec![Action::Move(position), Action::Press(position)]
            };
        }
        let Some(finger) = self.finger.as_mut().filter(|f| f.id == id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if phase == Phase::Move {
            let dx = position[0] - finger.start[0];
            let dy = position[1] - finger.start[1];
            if matches!(finger.motion, Motion::Pending) && dx.hypot(dy) >= 6.0 * scale {
                // Horizontal gestures adjust sliders. Holding before moving also preserves
                // inventory drag gestures, while a quick vertical movement scrolls a list.
                if dx.abs() > dy.abs()
                    || (finger.hold_drag
                        && now.duration_since(finger.started) >= Duration::from_millis(350))
                {
                    finger.motion = Motion::Drag;
                    out.push(Action::Move(finger.start));
                    out.push(Action::Press(finger.start));
                } else {
                    finger.motion = Motion::Scroll;
                }
            }
            match finger.motion {
                Motion::Scroll => out.push(Action::Scroll {
                    position,
                    lines: ((position[1] - finger.last[1]) / scale / 18.0) as f32,
                }),
                Motion::Pending | Motion::Drag => out.push(Action::Move(position)),
            }
            finger.last = position;
        } else {
            match finger.motion {
                Motion::Pending
                    if phase == Phase::End
                        && (position[0] - finger.start[0]).hypot(position[1] - finger.start[1])
                            < 6.0 * scale =>
                {
                    out.extend([
                        Action::Move(position),
                        Action::Press(position),
                        Action::Release(position),
                    ]);
                }
                Motion::Drag => out.extend([Action::Move(position), Action::Release(position)]),
                _ => (),
            }
            self.finger = None;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn swipes_scroll_without_clicking_and_ignore_other_fingers() {
        let mut g = MenuTouch::default();
        let t = Instant::now();
        g.update(1, Phase::Start, [100., 200.], 2., Mode::Scroll, t);
        assert!(
            g.update(2, Phase::End, [100., 200.], 2., Mode::Scroll, t)
                .is_empty()
        );
        let actions = g.update(
            1,
            Phase::Move,
            [100., 164.],
            2.,
            Mode::Scroll,
            t + Duration::from_millis(20),
        );
        assert_eq!(
            actions,
            vec![Action::Scroll {
                position: [100., 164.],
                lines: -1.
            }]
        );
        assert!(
            g.update(1, Phase::End, [100., 164.], 2., Mode::Scroll, t)
                .is_empty()
        );
    }
    #[test]
    fn taps_click_once_and_cancellation_never_clicks() {
        let mut g = MenuTouch::default();
        let t = Instant::now();
        g.update(1, Phase::Start, [10., 10.], 1., Mode::Scroll, t);
        assert_eq!(
            g.update(1, Phase::End, [11., 11.], 1., Mode::Scroll, t),
            vec![
                Action::Move([11., 11.]),
                Action::Press([11., 11.]),
                Action::Release([11., 11.])
            ]
        );
        g.update(1, Phase::Start, [10., 10.], 1., Mode::Scroll, t);
        assert!(
            g.update(1, Phase::Cancel, [10., 10.], 1., Mode::Scroll, t)
                .is_empty()
        );
    }
    #[test]
    fn holding_a_settings_row_still_scrolls_and_fast_swipes_never_click() {
        let t = Instant::now();
        let mut g = MenuTouch::default();
        g.update(1, Phase::Start, [20., 200.], 1., Mode::Scroll, t);
        assert!(
            g.update(
                1,
                Phase::Move,
                [20., 160.],
                1.,
                Mode::Scroll,
                t + Duration::from_secs(1)
            )
            .iter()
            .all(|a| matches!(a, Action::Scroll { .. }))
        );
        g.clear();
        g.update(1, Phase::Start, [20., 200.], 1., Mode::Scroll, t);
        assert!(
            g.update(1, Phase::End, [20., 40.], 1., Mode::Scroll, t)
                .is_empty()
        );
    }
    #[test]
    fn sliders_hold_drag_and_direct_inventory_keep_mouse_drag_semantics() {
        let t = Instant::now();
        for (position, time, mode) in [
            ([80., 10.], 20, Mode::Scroll),
            ([10., 80.], 400, Mode::ScrollAndDrag),
            ([10., 80.], 20, Mode::Direct),
        ] {
            let mut g = MenuTouch::default();
            let start = g.update(1, Phase::Start, [10., 10.], 1., mode, t);
            let movement = g.update(
                1,
                Phase::Move,
                position,
                1.,
                mode,
                t + Duration::from_millis(time),
            );
            assert!(
                start
                    .iter()
                    .chain(&movement)
                    .any(|a| matches!(a, Action::Press(_)))
            );
            assert!(!movement.iter().any(|a| matches!(a, Action::Scroll { .. })));
            assert!(
                g.update(1, Phase::Cancel, position, 1., mode, t)
                    .iter()
                    .any(|a| matches!(a, Action::Release(_)))
            );
        }
    }
}
