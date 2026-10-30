//! Native account UI state and private Android persistence. Only the UI thread commits a completed
//! worker's result, so cancelling or signing out cannot race with a background token refresh.
//! The caller must supply AndroidApp::internal_data_path(), never external_data_path().
use crate::microsoft_account::{self as msa, Account, DeviceCode, Endpoints, Secrets};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, Debug)]
pub enum State {
    Unconfigured,
    SignedOut,
    Starting,
    /// Show the code and a browser button. Never open a browser without the user's action.
    Code(DeviceCode),
    Refreshing,
    SignedIn(Account),
    Error(String),
}

enum Event {
    Code(DeviceCode),
    Done(Result<(Account, Secrets), String>),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Cached {
    client_id: String,
    #[serde(default)]
    verified_at: u64,
    account: Account,
    secrets: Secrets,
}

impl Cached {
    fn recent(&self, now: u64) -> bool {
        self.verified_at > 0
            && self.verified_at <= now
            && now - self.verified_at < 3600
            && self.secrets.expires_at() > now.saturating_add(300)
            && self.secrets.verified_session(&self.account).is_ok()
    }
}

struct Worker {
    cancelled: Arc<AtomicBool>,
    events: mpsc::Receiver<Event>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Account interaction is available on the title screen; do not switch player identity mid-world.
pub struct AccountManager {
    directory: PathBuf,
    client_id: Option<String>,
    state: State,
    worker: Option<Worker>,
    account: Option<Account>,
    verified: Option<(Account, Secrets)>,
    active: Arc<AtomicBool>,
    restored: bool,
}

impl AccountManager {
    pub fn new(internal_files: &Path) -> Result<Self, String> {
        let base = internal_files
            .canonicalize()
            .map_err(|_| "No se pudo abrir el almacenamiento privado de Android.")?;
        // Android's app-private path is authoritative. External/shared storage is never a fallback.
        #[cfg(target_os = "android")]
        if !(base.starts_with("/data/user") || base.starts_with("/data/data")) {
            return Err(
                "Las cuentas requieren el almacenamiento interno privado de Android.".into(),
            );
        }
        let directory = base.join("microsoft-account");
        if fs::symlink_metadata(&directory).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("El directorio privado de cuentas no es seguro.".into());
        }
        fs::create_dir_all(&directory)
            .map_err(|_| "No se pudo crear el almacenamiento privado de cuentas.")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| "No se pudo proteger el almacenamiento de cuentas.")?;
        }
        let configured = fs::read_to_string(directory.join("client-id"))
            .ok()
            .or_else(msa::client_id)
            .filter(|id| msa::valid_client_id(id));
        let mut this = Self {
            directory,
            state: if configured.is_some() {
                State::SignedOut
            } else {
                State::Unconfigured
            },
            client_id: configured,
            worker: None,
            account: None,
            verified: None,
            active: Arc::new(AtomicBool::new(true)),
            restored: false,
        };
        if let Some(id) = this.client_id.clone() {
            match this.read_cached() {
                Ok(Some(cached)) if cached.client_id == id => {
                    let now = unix_now();
                    if cached.recent(now) {
                        this.account = Some(cached.account.clone());
                        this.state = State::SignedIn(cached.account.clone());
                        this.verified = Some((cached.account, cached.secrets));
                        this.restored = true;
                    } else {
                        this.spawn(Some((cached.account, cached.secrets)));
                    }
                }
                Ok(_) => {}
                Err(error) => this.state = State::Error(error),
            }
        }
        Ok(this)
    }

    /// Android lifecycle updates this without cancelling a valid sign-in grant.
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }
    pub fn verified(&self) -> bool {
        self.verified
            .as_ref()
            .is_some_and(|(_, secrets)| secrets.expires_at() > unix_now())
    }

    fn revoke(&mut self) {
        self.restored = false;
        self.verified = None;
        self.account = None;
        crate::auth::clear_current();
    }
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some()
    }

    /// This public GUID belongs to the developer's Microsoft app; never accept a token here.
    /// Reconfiguration also removes cached credentials belonging to the previous client.
    pub fn configure(&mut self, client_id: &str) -> Result<(), String> {
        let id = client_id.trim();
        if !msa::valid_client_id(id) {
            return Err("El identificador de aplicación debe ser un GUID público válido.".into());
        }
        if self.client_id.as_deref() == Some(id) {
            return Ok(());
        }
        self.cancel();
        self.revoke();
        self.remove_cached()?;
        private_write(&self.directory.join("client-id"), id.as_bytes())?;
        self.client_id = Some(id.to_owned());
        self.account = None;
        self.state = State::SignedOut;
        Ok(())
    }

    pub fn sign_in(&mut self) {
        self.cancel();
        self.revoke();
        if self.client_id.is_none() {
            self.state = State::Unconfigured;
            return;
        }
        self.spawn(None);
    }

    fn spawn(&mut self, saved: Option<(Account, Secrets)>) {
        let Some(client_id) = self.client_id.clone() else {
            return;
        };
        self.state = if saved.is_some() {
            State::Refreshing
        } else {
            State::Starting
        };
        let (sender, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let active = self.active.clone();
        self.worker = Some(Worker { cancelled, events });
        std::thread::spawn(move || {
            let result = (|| {
                let ep = Endpoints::live();
                if let Some((account, secrets)) = saved {
                    return msa::verified_fresh(&ep, &client_id, &account, secrets);
                }
                let code = msa::device_code(&ep, &client_id)?;
                if flag.load(Ordering::Relaxed) {
                    return Err("Sign-in cancelled.".into());
                }
                let _ = sender.send(Event::Code(code.clone()));
                let tokens = msa::wait_for_sign_in_while_active(
                    &ep,
                    &client_id,
                    &code,
                    &|| flag.load(Ordering::Relaxed),
                    &|| active.load(Ordering::Relaxed),
                )?;
                if flag.load(Ordering::Relaxed) {
                    return Err("Sign-in cancelled.".into());
                }
                msa::finish_sign_in(&ep, tokens)
            })();
            if !flag.load(Ordering::Relaxed) {
                let _ = sender.send(Event::Done(result));
            }
        });
    }

    /// Call from the title/menu tick. A successful result must update Game's player name/UUID and
    /// auth::set_current. The profile in State::SignedIn carries the official skin/cape URLs.
    /// Persistence failures are shown but still permit this session in memory.
    pub fn tick(&mut self) -> Option<crate::auth::AuthSession> {
        if std::mem::take(&mut self.restored) {
            return self
                .verified
                .as_ref()
                .and_then(|(account, secrets)| secrets.verified_session(account).ok());
        }
        if self.worker.is_none()
            && self.active.load(Ordering::Relaxed)
            && let Some((account, secrets)) = &self.verified
            && secrets.expires_at() <= unix_now().saturating_add(300)
        {
            self.spawn(Some((account.clone(), secrets.clone())));
        }
        loop {
            let event = self.worker.as_ref()?.events.try_recv();
            match event {
                Ok(Event::Code(code)) => self.state = State::Code(code),
                Ok(Event::Done(result)) => {
                    self.worker = None;
                    match result {
                        Ok((account, secrets)) => {
                            let session = match secrets.verified_session(&account) {
                                Ok(session) => session,
                                Err(error) => {
                                    self.revoke();
                                    self.state = State::Error(error);
                                    return None;
                                }
                            };
                            if !session.java_verified() {
                                self.revoke();
                                self.state = State::Error(
                                    "La sesión de Minecraft venció. Iniciá sesión de nuevo.".into(),
                                );
                                return None;
                            }
                            self.account = Some(account.clone());
                            self.verified = Some((account.clone(), secrets.clone()));
                            let cached = Cached {
                                client_id: self.client_id.clone().unwrap_or_default(),
                                verified_at: unix_now(),
                                account: account.clone(),
                                secrets,
                            };
                            self.state = match serde_json::to_vec(&cached)
                                .map_err(|_| "No se pudo guardar la sesión privada.".to_owned())
                                .and_then(|bytes| {
                                    private_write(&self.directory.join("session.json"), &bytes)
                                }) {
                                Ok(()) => State::SignedIn(account),
                                Err(_) => State::Error(format!(
                                    "Conectado como {}. No se pudo guardar la sesión; tendrás que iniciar sesión al reabrir.",
                                    account.name
                                )),
                            };
                            return Some(session);
                        }
                        Err(error) => {
                            self.revoke();
                            self.state = State::Error(error);
                        }
                    }
                    return None;
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.revoke();
                    self.state = State::Error(
                        "Se interrumpió el inicio de sesión. Intentá de nuevo.".into(),
                    );
                    return None;
                }
            }
        }
    }

    pub fn cancel(&mut self) {
        self.worker = None;
        if matches!(
            self.state,
            State::Starting | State::Code(_) | State::Refreshing
        ) {
            self.state = if self.client_id.is_some() {
                State::SignedOut
            } else {
                State::Unconfigured
            };
        }
    }

    /// Caller also clears the in-memory auth session and resets the offline name/UUID/skin.
    pub fn sign_out(&mut self) -> Result<(), String> {
        self.cancel();
        // Close the in-memory gate even if a storage failure prevents deleting the cache.
        self.revoke();
        let removed = self.remove_cached();
        self.state = if self.client_id.is_some() {
            State::SignedOut
        } else {
            State::Unconfigured
        };
        removed
    }

    fn remove_cached(&self) -> Result<(), String> {
        match fs::remove_file(self.directory.join("session.json")) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("No se pudo eliminar la sesión privada de este dispositivo.".into()),
        }
    }

    fn read_cached(&self) -> Result<Option<Cached>, String> {
        let path = self.directory.join("session.json");
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("No se pudo leer la sesión privada.".into()),
        };
        if !meta.is_file() || meta.len() > 64 * 1024 {
            return Err("La sesión privada no es válida. Iniciá sesión de nuevo.".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(
                    "La sesión guardada no tiene permisos privados. Iniciá sesión de nuevo.".into(),
                );
            }
        }
        let bytes = fs::read(path).map_err(|_| "No se pudo leer la sesión privada.")?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "La sesión privada está dañada. Iniciá sesión de nuevo.".into())
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension("new");
    // create_new refuses a leftover file or symlink. Remove only our same-directory temp entry.
    let _ = fs::remove_file(&temporary);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|_| "No se pudo guardar la sesión en almacenamiento privado.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory() -> PathBuf {
        static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "horizon-account-test-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn verified_result(expires_at: u64) -> (Account, Secrets) {
        let account = Account {
            uuid: "069a79f444e94726a5befca90e38aaf5".into(),
            name: "Tester".into(),
            skin: None,
            cape: None,
        };
        let secrets = serde_json::from_value(serde_json::json!({
            "refresh_token": "mock-refresh", "minecraft_token": "mock-access", "minecraft_expires_at": expires_at
        })).unwrap();
        (account, secrets)
    }
    fn completed_worker(manager: &mut AccountManager, result: Result<(Account, Secrets), String>) {
        let (sender, events) = mpsc::channel();
        manager.worker = Some(Worker {
            cancelled: Arc::new(AtomicBool::new(false)),
            events,
        });
        sender.send(Event::Done(result)).unwrap();
    }
    #[test]
    fn verified_cache_is_bounded_by_age_and_token_expiry() {
        let (account, secrets) = verified_result(20_000);
        let mut cached = Cached {
            client_id: "test".into(),
            verified_at: 10_000,
            account,
            secrets,
        };
        assert!(cached.recent(10_100));
        assert!(!cached.recent(9_999));
        assert!(!cached.recent(13_600));
        cached.verified_at = 19_900;
        assert!(!cached.recent(19_900));
        cached.verified_at = 0;
        assert!(!cached.recent(1));
    }
    #[test]
    fn recently_verified_private_session_restores_without_a_network_worker() {
        let root = directory();
        let mut manager = AccountManager::new(&root).unwrap();
        manager
            .configure("3908277c-d619-4fc3-bc07-0c72b38ae12e")
            .unwrap();
        completed_worker(&mut manager, Ok(verified_result(unix_now() + 7200)));
        assert!(manager.tick().is_some());
        drop(manager);
        let mut restored = AccountManager::new(&root).unwrap();
        assert!(!restored.busy());
        assert!(restored.verified());
        assert!(restored.tick().unwrap().java_verified());
        restored.sign_out().unwrap();
        assert!(!restored.verified());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn gameplay_gate_requires_live_verified_result_and_revokes_on_failure() {
        let root = directory();
        let mut manager = AccountManager::new(&root).unwrap();
        assert!(!manager.verified());
        completed_worker(&mut manager, Ok(verified_result(unix_now() + 7200)));
        assert!(manager.tick().unwrap().java_verified());
        assert!(manager.verified());
        completed_worker(&mut manager, Err("ownership revoked".into()));
        assert!(manager.tick().is_none());
        assert!(!manager.verified());
        assert!(manager.account().is_none());
        completed_worker(&mut manager, Ok(verified_result(1)));
        assert!(manager.tick().is_none());
        assert!(!manager.verified());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn signout_closes_gate_even_when_cache_cannot_be_deleted() {
        let root = directory();
        let mut manager = AccountManager::new(&root).unwrap();
        completed_worker(&mut manager, Ok(verified_result(unix_now() + 7200)));
        assert!(manager.tick().is_some());
        let cache = manager.directory.join("session.json");
        fs::remove_file(&cache).unwrap();
        fs::create_dir(&cache).unwrap();
        assert!(manager.sign_out().is_err());
        assert!(!manager.verified());
        assert!(manager.account().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configuration_and_sign_out_never_keep_credentials() {
        let root = directory();
        let mut manager = AccountManager::new(&root).unwrap();
        assert!(!manager.busy());
        assert!(manager.configure("not-a-token-or-id").is_err());
        let id = "12345678-abcd-1234-abcd-1234567890ab";
        manager.configure(id).unwrap();
        assert_eq!(manager.client_id(), Some(id));
        private_write(&manager.directory.join("session.json"), b"mock-secrets").unwrap();
        manager.sign_out().unwrap();
        assert!(!manager.directory.join("session.json").exists());
        assert_eq!(
            fs::read_to_string(manager.directory.join("client-id")).unwrap(),
            id
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn storage_is_atomic_private_and_refuses_symlinked_cache() {
        let root = directory();
        let manager = AccountManager::new(&root).unwrap();
        let path = manager.directory.join("session.json");
        private_write(&path, b"first").unwrap();
        private_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!path.with_extension("new").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            fs::remove_file(&path).unwrap();
            symlink(root.join("anything"), &path).unwrap();
            assert!(manager.read_cached().is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancelled_worker_cannot_publish_or_persist_a_late_result() {
        let root = directory();
        let mut manager = AccountManager::new(&root).unwrap();
        let (sender, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        manager.worker = Some(Worker {
            cancelled: cancelled.clone(),
            events,
        });
        manager.state = State::Starting;
        manager.cancel();
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(sender.send(Event::Done(Err("late".into()))).is_err());
        assert!(manager.tick().is_none());
        assert!(!manager.directory.join("session.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
