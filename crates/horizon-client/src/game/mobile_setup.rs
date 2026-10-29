//! Account-first startup with original artwork/font, usable before any Mojang file exists.
use super::*;
use horizon_client::{auth, hud::rgba, text::Text, ui};
use std::sync::{Mutex, atomic::AtomicBool, mpsc};

pub(super) struct Setup {
    pub window: Option<Arc<Window>>,
    pub renderer: Option<Renderer>,
    gui: UiResources,
    settings: Settings,
    options: Option<Options>,
    account: mobile_account::AccountUi,
    internal: PathBuf,
    helper: PathBuf,
    progress: Arc<Mutex<String>>,
    cancel: Arc<AtomicBool>,
    worker: Option<mpsc::Receiver<std::result::Result<PathBuf, String>>>,
    prepared: Option<PathBuf>,
    failure: Option<String>,
    cursor: (f32, f32),
    pub error: Option<Box<dyn std::error::Error>>,
}
impl Drop for Setup {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Setup {
    pub fn new(options: Options, settings: Settings) -> Result<Self> {
        // Android may expose this OS-owned directory through /data/user/0 -> /data/data.
        // Resolve only that trusted anchor; downloader paths below it still reject symlinks.
        let internal = std::fs::canonicalize(
            crate::android_app()
                .internal_data_path()
                .ok_or("No private installation directory")?,
        )?;
        let root = super::mobile_bootstrap_art::original_artwork(&internal)?;
        let mut account = mobile_account::AccountUi::default();
        account.set_mandatory(true);
        horizon_client::lang::set(horizon_client::lang::Language::load(
            &[&root],
            &settings.language,
        ));
        horizon_client::hud::GUI_SCALE.store(settings.gui_scale, Ordering::Relaxed);
        Ok(Self {
            window: None,
            renderer: None,
            gui: UiResources::load(&root, &root)?,
            settings,
            options: Some(options),
            account,
            internal,
            helper: crate::android_native_library_dir()?.join("libhorizon_java.so"),
            progress: Arc::new(Mutex::new(super::mobile_locale::tr("verify_first"))),
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
            prepared: None,
            failure: None,
            cursor: (0., 0.),
            error: None,
        })
    }
    pub fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.account.set_active(true);
        let result = (|| -> Result<()> {
            if let Some(renderer) = &mut self.renderer {
                renderer.recreate_surface()?;
                renderer.suspended = false;
                return Ok(());
            }
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Horizon")
                        .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None))),
                )?,
            );
            self.renderer = Some(loading_renderer(
                window.clone(),
                false,
                Some(&self.gui),
                None,
                self.settings.horizon.api,
            )?);
            self.window = Some(window);
            Ok(())
        })();
        if let Err(e) = result {
            self.error = Some(e);
            event_loop.exit();
        }
    }
    pub fn suspended(&mut self) {
        self.account.set_active(false);
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
            renderer.suspended = true;
        }
    }
    pub fn event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        let event = mobile::translate_event(event);
        let size = self
            .renderer
            .as_ref()
            .map_or([1280, 720], |r| [r.config.width, r.config.height]);
        if let WindowEvent::Touch(touch) = &event {
            self.event(
                event_loop,
                id,
                WindowEvent::CursorMoved {
                    device_id: touch.device_id,
                    position: touch.location,
                },
            );
            if matches!(
                touch.phase,
                winit::event::TouchPhase::Started
                    | winit::event::TouchPhase::Ended
                    | winit::event::TouchPhase::Cancelled
            ) {
                self.event(
                    event_loop,
                    id,
                    WindowEvent::MouseInput {
                        device_id: touch.device_id,
                        state: if touch.phase == winit::event::TouchPhase::Started {
                            ElementState::Pressed
                        } else {
                            ElementState::Released
                        },
                        button: winit::event::MouseButton::Left,
                    },
                );
            }
            return;
        }
        if let WindowEvent::Focused(active) = &event {
            self.account.set_active(*active);
        }
        if let WindowEvent::CursorMoved { position, .. } = &event {
            let b = HudBatch::new(size);
            self.cursor = (position.x as f32 / b.scale, position.y as f32 / b.scale);
        }
        if !auth::android_verified() {
            self.account.open = true;
            self.account.event(&event, size);
            if self.account.take_exit() {
                self.finish(event_loop);
                return;
            }
        } else if self.failure.is_some()
            && matches!(
                &event,
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: winit::event::MouseButton::Left,
                    ..
                }
            )
        {
            let batch = HudBatch::new(size);
            if ui::hovered(self.cursor, 20., batch.height - 42., 150., 24.) {
                self.failure = None;
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                self.finish(event_loop);
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => (),
        }
    }
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Err(error) = crate::android_finish() {
            log::error!("Could not finish Android activity: {error}");
        }
        event_loop.exit();
    }
    pub fn poll(&mut self, event_loop: &ActiveEventLoop) -> Result<Option<Loading>> {
        if self.error.is_some() {
            self.cancel.store(true, Ordering::Relaxed);
            event_loop.exit();
            return Ok(None);
        }
        self.account.poll();
        if !auth::android_verified() {
            self.account.open = true;
            if self.worker.is_some() {
                self.cancel.store(true, Ordering::Relaxed);
            }
        } else {
            self.account.open = false;
            if self.worker.is_none() && self.prepared.is_none() && self.failure.is_none() {
                self.cancel = Arc::new(AtomicBool::new(false));
                let (tx, rx) = mpsc::channel();
                let (internal, helper, cancel, progress) = (
                    self.internal.clone(),
                    self.helper.clone(),
                    self.cancel.clone(),
                    self.progress.clone(),
                );
                std::thread::Builder::new()
                    .name("android-installer".into())
                    .spawn(move || {
                        let result = super::mobile_setup_worker::prepare(
                            &internal,
                            &helper,
                            &cancel,
                            &|line| {
                                *progress.lock().unwrap_or_else(|e| e.into_inner()) = line;
                            },
                        );
                        let _ = tx.send(result);
                    })?;
                self.worker = Some(rx);
            }
        }
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(Ok(root)) => {
                    self.prepared = Some(root);
                    self.worker = None;
                }
                Ok(Err(e)) => {
                    self.failure = Some(e);
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failure = Some(super::mobile_locale::tr("installer_failed"));
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => (),
            }
        }
        if auth::android_verified()
            && let Some(root) = self.prepared.take()
        {
            let mut options = self.options.take().ok_or("Setup already finished")?;
            options.resources = root.clone();
            let progress = Progress::new();
            let started = Instant::now();
            let (receiver, handle) =
                startup_menu::spawn_loader(&options, &self.settings, &root, started, &progress)?;
            let mut menu = startup_menu::StartupMenu::new(&options, None)?;
            self.account.set_mandatory(false);
            menu.account = std::mem::take(&mut self.account);
            return Ok(Some(Loading {
                window: self.window.take(),
                renderer: self.renderer.take(),
                gui: None,
                settings: self.settings.clone(),
                fullscreen: horizon_client::window::FullscreenState::new(true),
                options: Some(options),
                vanilla_root: root,
                pack_error: None,
                started,
                panorama_rotation: Default::default(),
                progress,
                receiver,
                loaded: None,
                handle: Some(handle),
                shown: false,
                menu,
                error: None,
            }));
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(50),
        ));
        Ok(None)
    }
    fn draw(&mut self) {
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let mut batch = HudBatch::new(size);
        let atlas = &self.gui.atlas;
        batch.fill(
            atlas,
            0.,
            0.,
            batch.width,
            batch.height,
            rgba(18, 20, 30, 255),
        );
        if !auth::android_verified() {
            self.account.open = true;
            self.account.draw(&mut batch, atlas, size);
        } else {
            batch.text(
                atlas,
                &super::mobile_locale::tr("installation"),
                20.,
                20.,
                rgba(255, 255, 255, 255),
                false,
            );
            let message = self.failure.clone().unwrap_or_else(|| {
                self.progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone()
            });
            let mut row = 0;
            let mut line = String::new();
            for word in message.split_whitespace() {
                if HudBatch::text_width(atlas, &format!("{line} {word}")) > batch.width - 40. {
                    batch.text(
                        atlas,
                        &line,
                        20.,
                        55. + row as f32 * 12.,
                        rgba(215, 216, 230, 255),
                        false,
                    );
                    row += 1;
                    line.clear();
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            batch.text(
                atlas,
                &line,
                20.,
                55. + row as f32 * 12.,
                rgba(215, 216, 230, 255),
                false,
            );
            if self.failure.is_some() {
                ui::button(
                    &mut batch,
                    atlas,
                    20.,
                    size[1] as f32 / HudBatch::new(size).scale - 42.,
                    150.,
                    24.,
                    &Text::literal(super::mobile_locale::tr("retry")),
                    true,
                    false,
                );
            }
        }
        if let Some(renderer) = &mut self.renderer
            && let Err(e) = renderer.render(&loading_frame(&batch))
        {
            self.error = Some(e);
        }
    }
}
