//! State shared by Android's native keyboard and the focused game field.
//! Keep the decision separate from platform calls so dismissal/reopening can be tested.
#[derive(Default)]
pub struct KeyboardFocus {
    field: Option<(&'static str, usize)>,
}
impl KeyboardFocus {
    /// `retap` reopens a keyboard the user dismissed with Android Back while
    /// the same edit box still has focus. Ordinary redraws must not reopen it.
    pub fn update(&mut self, field: Option<(&'static str, usize)>, retap: bool) -> Option<bool> {
        let changed = self.field != field;
        self.field = field;
        if changed || (retap && field.is_some()) {
            Some(field.is_some())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opens_on_focus_hides_on_blur_without_repeating_per_frame() {
        let mut focus = KeyboardFocus::default();
        assert_eq!(focus.update(None, false), None);
        assert_eq!(focus.update(Some(("server.address", 0)), false), Some(true));
        assert_eq!(focus.update(Some(("server.address", 0)), false), None);
        assert_eq!(focus.update(None, false), Some(false));
        assert_eq!(focus.update(None, false), None);
    }
    #[test]
    fn retapping_reopens_after_back_and_switching_fields_keeps_input_open() {
        let mut focus = KeyboardFocus::default();
        focus.update(Some(("server.name", 0)), false);
        assert_eq!(focus.update(Some(("server.name", 0)), true), Some(true));
        assert_eq!(focus.update(Some(("server.address", 0)), false), Some(true));
        assert_eq!(focus.update(None, true), Some(false));
        assert_eq!(focus.update(None, true), None);
    }
}
