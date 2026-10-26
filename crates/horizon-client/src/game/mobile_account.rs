//! Android account UI: one player-facing sign-in action, with app identity configured internally.
use super::{
    Game, Screen,
    mobile_locale::{localize_error, tr},
};
use horizon_client::{
    android_account::{AccountManager, State},
    auth,
    hud::{HudAtlas, HudBatch, rgba},
    microsoft_account::{Account, DeviceCode},
    text::Text,
    ui,
};
use std::sync::Arc;
use winit::{
    event::{ElementState, MouseButton, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};

// Public application identity registered by the owner of this Android port.
// It is configuration, never a credential. Minecraft approval is separate.
pub(super) const ANDROID_CLIENT_ID: &str = "3908277c-d619-4fc3-bc07-0c72b38ae12e";

#[derive(Default)]
pub(super) struct AccountUi {
    manager: Option<AccountManager>,
    error: Option<String>,
    pub(super) open: bool,
    mandatory: bool,
    exit_requested: bool,
    cursor: (f32, f32),
    browser_pending: bool,
    code_copied: bool,
    inactive: bool,
    pub(super) profile: Option<Account>,
}
impl AccountUi {
    pub(super) fn set_mandatory(&mut self, mandatory: bool) {
        self.mandatory = mandatory;
    }
    pub(super) fn take_exit(&mut self) -> bool {
        std::mem::take(&mut self.exit_requested)
    }
    pub(super) fn require_sign_in(&mut self) -> bool {
        if auth::android_verified() {
            return true;
        }
        self.poll();
        if auth::android_verified() {
            return true;
        }
        self.open = true;
        false
    }
    fn initialize(&mut self) {
        if self.manager.is_some() || self.error.is_some() {
            return;
        }
        let result = crate::android_app()
            .internal_data_path()
            .ok_or_else(|| "Android no dio acceso al almacenamiento privado.".to_owned())
            .and_then(|path| AccountManager::new(&path))
            .and_then(|mut manager| {
                manager.configure(ANDROID_CLIENT_ID)?;
                Ok(manager)
            });
        match result {
            Ok(manager) => {
                manager.set_active(!self.inactive);
                self.manager = Some(manager);
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn set_active(&mut self, active: bool) {
        self.inactive = !active;
        if let Some(manager) = &self.manager {
            manager.set_active(active);
        }
    }
    pub(super) fn poll(&mut self) {
        self.initialize();
        if let Some(manager) = &mut self.manager
            && let Some(session) = manager.tick()
        {
            self.profile = manager.account().cloned();
            auth::set_current(session);
        }
        if self
            .manager
            .as_ref()
            .is_some_and(|manager| !manager.verified())
        {
            self.profile = None;
            auth::clear_current();
        }
        // The explicit sign-in press authorizes this browser launch. A redraw,
        // refresh, cancellation or a closed panel never opens it independently.
        if self.open && self.browser_pending {
            let state = self.manager.as_ref().map(|m| m.state().clone());
            match state {
                Some(State::Code(code)) => {
                    self.browser_pending = false;
                    self.browser(&code);
                }
                Some(State::Error(_) | State::SignedIn(_)) => self.browser_pending = false,
                _ => {}
            }
        }
    }
    fn start_sign_in(&mut self) {
        self.initialize();
        if let Some(manager) = &mut self.manager {
            if manager.busy() {
                return;
            }
            self.error = None;
            self.code_copied = false;
            self.browser_pending = true;
            manager.sign_in();
        }
    }
    fn layout(size: [u32; 2]) -> ([f32; 4], [f32; 4]) {
        let batch = HudBatch::new(size);
        let width = (batch.width - 24.).clamp(200., 394.);
        let height = (batch.height - 16.).clamp(150., 190.);
        (
            [batch.width - 112., 8., 104., 22.],
            [
                ((batch.width - width) / 2.).floor(),
                ((batch.height - height) / 2.).floor(),
                width,
                height,
            ],
        )
    }
    fn secondary_visible(&self) -> bool {
        !self.mandatory || self.manager.as_ref().is_some_and(AccountManager::busy)
    }
    fn primary_width(&self, width: f32) -> f32 {
        width - if self.secondary_visible() { 146. } else { 28. }
    }
    fn in_rect(&self, r: [f32; 4]) -> bool {
        ui::hovered(self.cursor, r[0], r[1], r[2], r[3])
    }
    fn close(&mut self) {
        if let Some(manager) = &mut self.manager {
            manager.cancel();
        }
        self.browser_pending = false;
        self.open = false;
        self.error = None;
        if self.mandatory {
            self.exit_requested = true;
        }
    }
    fn browser(&mut self, code: &DeviceCode) {
        if let Some(url) = code.browser_url() {
            // Only the short user-facing linking code reaches the clipboard.
            // Device codes and access/refresh tokens remain private.
            self.code_copied = crate::android_copy_link_code(&code.user_code).is_ok();
            if let Err(error) = crate::android_open_browser(url) {
                self.error = Some(error);
            }
        } else {
            self.error =
                Some("Microsoft devolvió una dirección de verificación no reconocida.".into());
        }
    }
    pub(super) fn event(&mut self, event: &WindowEvent, size: [u32; 2]) -> bool {
        if let WindowEvent::CursorMoved { position, .. } = event {
            let batch = HudBatch::new(size);
            self.cursor = (
                position.x as f32 / batch.scale,
                position.y as f32 / batch.scale,
            );
            return self.open;
        }
        let (account_button, [x, y, width, height]) = Self::layout(size);
        if !self.open {
            if matches!(
                event,
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                }
            ) && self.in_rect(account_button)
            {
                self.open = true;
                self.initialize();
                if !auth::android_verified() {
                    self.start_sign_in();
                }
                return true;
            }
            return false;
        }
        match event {
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let bottom = y + height - 32.;
                let right = [x + width - 126., bottom, 112., 24.];
                if self.in_rect([x + width - 30., y + 7., 22., 20.]) {
                    self.close();
                } else if self.in_rect([x + 14., bottom, self.primary_width(width), 24.]) {
                    let code = self.manager.as_ref().and_then(|m| match m.state() {
                        State::Code(code) => Some(code.clone()),
                        _ => None,
                    });
                    if let Some(code) = code {
                        self.browser(&code);
                    } else if auth::android_verified() {
                        self.close();
                    } else {
                        self.start_sign_in();
                    }
                } else if self.in_rect(right) && self.secondary_visible() {
                    self.browser_pending = false;
                    if let Some(manager) = &mut self.manager {
                        if manager.busy() {
                            manager.cancel();
                        } else if auth::android_verified() {
                            match manager.sign_out() {
                                Ok(()) => {
                                    self.profile = None;
                                    auth::clear_current();
                                    self.error = None;
                                }
                                Err(error) => {
                                    self.profile = None;
                                    self.error = Some(error);
                                }
                            }
                        } else {
                            self.close();
                        }
                    } else {
                        self.close();
                    }
                }
                true
            }
            WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Ime(_) => true,
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => self.close(),
                        PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                            if auth::android_verified() {
                                self.close();
                            } else {
                                self.start_sign_in();
                            }
                        }
                        _ => {}
                    }
                }
                true
            }
            _ => false,
        }
    }
    pub(super) fn draw(&self, batch: &mut HudBatch, atlas: &HudAtlas, size: [u32; 2]) {
        let (account_button, [x, y, width, height]) = Self::layout(size);
        let signed_in = auth::current().filter(|session| session.java_verified());
        if !self.open {
            let label = signed_in
                .as_ref()
                .map_or_else(sign_in_label, |s| s.name.clone());
            let [x, y, w, h] = account_button;
            ui::button(
                batch,
                atlas,
                x,
                y,
                w,
                h,
                &Text::literal(label),
                true,
                self.in_rect(account_button),
            );
            return;
        }
        batch.fill(
            atlas,
            0.,
            0.,
            batch.width,
            batch.height,
            rgba(9, 12, 20, 215),
        );
        batch.fill(atlas, x, y, width, height, rgba(24, 27, 39, 250));
        batch.fill(atlas, x, y, width, 2., rgba(154, 134, 255, 255));
        batch.text(atlas, &tr("title"), x + 14., y + 14., u32::MAX, false);
        batch.text(
            atlas,
            &tr("subtitle"),
            x + 14.,
            y + 29.,
            rgba(176, 184, 205, 255),
            false,
        );
        button(
            batch,
            atlas,
            [x + width - 30., y + 7., 22., 20.],
            "X",
            false,
            true,
        );
        let state = self.manager.as_ref().map(AccountManager::state);
        let status = self
            .error
            .as_deref()
            .map(localize_error)
            .unwrap_or_else(|| match state {
                None => tr("storage"),
                Some(State::Unconfigured) => tr("unconfigured"),
                Some(State::SignedOut) => tr("signed_out"),
                Some(State::Starting) => tr("starting"),
                Some(State::Refreshing) => tr("refreshing"),
                Some(State::Code(_)) => tr("browser"),
                Some(State::SignedIn(account)) => tr("signed_in").replacen("{}", &account.name, 1),
                Some(State::Error(error)) => localize_error(error),
            });
        wrapped(
            batch,
            atlas,
            &status,
            x + 14.,
            y + 49.,
            width - 28.,
            3,
            rgba(217, 221, 234, 255),
        );
        if let Some(State::Code(code)) = state {
            let label = tr("code").replacen("{}", &code.user_code, 1);
            batch.text(
                atlas,
                &label,
                x + 14.,
                y + 87.,
                rgba(202, 189, 255, 255),
                false,
            );
            wrapped(
                batch,
                atlas,
                &tr(if self.code_copied {
                    "copied"
                } else {
                    "code_help"
                }),
                x + 14.,
                y + 102.,
                width - 28.,
                2,
                rgba(176, 184, 205, 255),
            );
        } else {
            wrapped(
                batch,
                atlas,
                &tr("privacy"),
                x + 14.,
                y + 90.,
                width - 28.,
                3,
                rgba(176, 184, 205, 255),
            );
        }
        let busy = self.manager.as_ref().is_some_and(AccountManager::busy);
        let is_code = matches!(state, Some(State::Code(_)));
        let bottom = y + height - 32.;
        button(
            batch,
            atlas,
            [x + 14., bottom, self.primary_width(width), 24.],
            if is_code {
                "Abrir Microsoft"
            } else if signed_in.is_some() {
                "Continuar"
            } else {
                "Iniciar sesión con Microsoft"
            },
            false,
            self.manager.is_some() && (!busy || is_code),
        );
        if self.secondary_visible() {
            button(
                batch,
                atlas,
                [x + width - 126., bottom, 112., 24.],
                if busy {
                    "Cancelar"
                } else if signed_in.is_some() {
                    "Cerrar sesión"
                } else {
                    "Volver"
                },
                false,
                true,
            );
        }
    }
}

fn button(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    rect: [f32; 4],
    label: &str,
    hovered: bool,
    enabled: bool,
) {
    let [x, y, w, h] = rect;
    let label = match label {
        "Iniciar sesión" => tr("sign_in"),
        "Iniciar sesión con Microsoft" => tr("sign_in_microsoft"),
        "Continuar" => tr("continue"),
        "Cancelar" => tr("cancel"),
        "Volver" => tr("back"),
        "Abrir Microsoft" => tr("open_microsoft"),
        "Cerrar sesión" => tr("sign_out"),
        "Salir" => tr("exit"),
        label => label.to_owned(),
    };
    ui::button(
        batch,
        atlas,
        x,
        y,
        w,
        h,
        &Text::literal(label),
        enabled,
        hovered,
    );
}

/// The whole panel uses the same selected language, including bootstrap fallback.
fn sign_in_label() -> String {
    tr("sign_in")
}

fn wrapped(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    lines: usize,
    color: u32,
) {
    let mut line = String::new();
    let mut row = 0;
    for word in text.split_whitespace() {
        let next = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if HudBatch::text_width(atlas, &next) > width && !line.is_empty() {
            batch.text(atlas, &line, x, y + row as f32 * 10.0, color, false);
            row += 1;
            if row == lines {
                return;
            }
            line = word.to_owned();
        } else {
            line = next;
        }
    }
    if row < lines {
        batch.text(atlas, &line, x, y + row as f32 * 10.0, color, false);
    }
}

impl Game<'_> {
    pub(super) fn mobile_account_event(&mut self, event: &WindowEvent) -> bool {
        if !self.mobile_account.open
            && (self
                .title
                .as_ref()
                .is_none_or(|title| !matches!(title.mode, horizon_client::title::Mode::Main))
                || self.screen != Screen::Playing
                || self.pending_entry.is_some())
        {
            return false;
        }
        let size = self.window.as_ref().map_or([1280, 720], |window| {
            let s = window.inner_size();
            [s.width, s.height]
        });
        let consumed = self.mobile_account.event(event, size);
        if consumed && let WindowEvent::CursorMoved { position, .. } = event {
            let batch = HudBatch::new(size);
            self.cursor = (
                position.x as f32 / batch.scale,
                position.y as f32 / batch.scale,
            );
        }
        consumed
    }
    pub(super) fn mobile_account_poll(&mut self) {
        self.mobile_account.poll();
        if !auth::android_verified() && self.title.is_some() {
            self.mobile_account.open = true;
        }
        if self.remote.is_none() && self.title.is_none() {
            let url = self
                .mobile_account
                .profile
                .as_ref()
                .and_then(Account::skin_url);
            if let Some(url) = url {
                let skin = self
                    .skin_downloads
                    .borrow_mut()
                    .state(horizon_client::skins::Kind::Skin, &url);
                if skin == horizon_client::skins::Download::Ready
                    && let Some(hash) = horizon_client::skins::texture_hash(&url)
                {
                    let location = horizon_client::skins::skin_location(&hash);
                    if self.show_skin(&location)
                        && let Some(model) = &mut self.player_model
                    {
                        model.skin = Some(location);
                    }
                }
            }
        }
    }
    /// The local player's arm geometry follows the authenticated active skin's variant.
    pub(super) fn mobile_player_kind(
        &self,
        kind: Arc<horizon_client::mobs::MobType>,
    ) -> Arc<horizon_client::mobs::MobType> {
        let slim = self
            .mobile_account
            .profile
            .as_ref()
            .and_then(|profile| profile.skin.as_ref())
            .is_some_and(|(_, variant)| variant == "slim");
        if !slim {
            return kind;
        }
        let Some(model) =
            horizon_client::mobs::load_model(&self.options.resources, "minecraft:player_slim#main")
        else {
            return kind;
        };
        let mut replacement = (*kind).clone();
        replacement.model = Arc::new(model);
        Arc::new(replacement)
    }
}
