//! Focus-driven Android software keyboard. NativeActivity's TYPE_NULL input
//! connection delivers ordinary typing and delete as keyboard events; do not
//! synthesize those again here. Accept committed strings for IME-capable backends.
use super::{Game, Screen};
use horizon_client::clipboard::FieldId;
use std::cell::{Cell, RefCell};
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    window::Window,
};

#[path = "../mobile_ime_state.rs"]
mod state;
thread_local! {
    static FOCUS: RefCell<state::KeyboardFocus> = RefCell::new(state::KeyboardFocus::default());
    static ACTIVE: Cell<bool> = const { Cell::new(true) };
}

/// Observe before dispatch; synchronize after dispatch, once a tap has selected
/// its edit box. Calling showSoftInput every frame would reopen it after Back.
pub(super) fn observe(event: &WindowEvent) -> bool {
    if let WindowEvent::Focused(focused) = event {
        ACTIVE.with(|active| active.set(*focused));
    }
    matches!(
        event,
        WindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Left,
            ..
        }
    )
}
pub(super) fn sync(window: &Window, field: Option<FieldId>, retap: bool) {
    let field = ACTIVE.with(|active| active.get().then_some(field).flatten());
    if let Some(show) = FOCUS.with(|focus| focus.borrow_mut().update(field, retap)) {
        window.set_ime_allowed(show);
    }
}

impl Game<'_> {
    pub(super) fn mobile_text_focus(&mut self) -> Option<FieldId> {
        if self.mobile_account.open {
            return None;
        }
        if self.server_packs.prompt.is_some() || self.pending_entry.is_some() {
            return None;
        }
        if self.screen == Screen::Options {
            return self
                .ui
                .options
                .text_input_focused()
                .then_some(("game.options", 0));
        }
        if let Some(title) = &mut self.title {
            return title.focused_field().map(|(id, _, _)| id);
        }
        if self.sign_edit.is_some() {
            return Some(("sign", 0));
        }
        if matches!(self.book, Some(super::books::BookScreen::Edit { .. })) {
            return Some(("book", 0));
        }
        let focused = match self.screen {
            Screen::Chat => true,
            Screen::Creative => self.creative_screen.tab().search,
            Screen::Options => self.ui.options.text_input_focused(),
            Screen::WorldOptions => self.mobile_world_options_typing(),
            Screen::CommandBlock => self.command_block_screen.is_some(),
            _ => self.recipe_search_focused() || self.station_typing(),
        };
        focused.then_some(("game.text", self.screen as usize))
    }
    pub(super) fn mobile_sync_ime(&mut self, retap: bool) {
        let field = self.mobile_text_focus();
        if let Some(window) = &self.window {
            sync(window, field, retap);
        }
    }
    pub(super) fn mobile_ime_commit(&mut self, text: &str) {
        if self.mobile_text_focus().is_none() {
            return;
        }
        if self.screen == Screen::CommandBlock {
            self.command_block_type(text);
        } else {
            self.pad_osk_output(horizon_client::osk::Output::Text(text.to_owned()));
        }
    }
}
