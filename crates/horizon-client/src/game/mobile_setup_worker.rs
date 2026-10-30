//! First-run preparation. Proprietary inputs are fetched on the device, never embedded.
use super::mobile_install_progress::{Event, Status};
use horizon_client::{auth, official_downloads};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, TryLockError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

include!(concat!(env!("OUT_DIR"), "/prepare_sources.rs"));

const RUNTIME_URL: &str = "https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/download/download_jre25/jre25-android-arm64.tar.xz";
const RUNTIME_HASH: &str = "d3eb7afe2240c26728a1bb440502c5f18ac3883e932d202dd7f0c9bcbbce4c37";
const RUNTIME_SOURCE: &str = "https://github.com/AngelAuraMC/angelauramc-openjdk-build";
static INSTALLER: Mutex<()> = Mutex::new(());

fn check(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) || !auth::android_verified() {
        Err("La instalación requiere una cuenta de Minecraft Java verificada.".into())
    } else {
        Ok(())
    }
}

/// Android may suspend the UI while an exporter is running. Expiry must still kill it.
struct ProofWatch {
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl ProofWatch {
    fn new(cancel: Arc<AtomicBool>) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        let thread = std::thread::spawn(move || {
            while !finished.load(Ordering::Relaxed) {
                if !auth::android_verified() {
                    cancel.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        Self {
            done,
            thread: Some(thread),
        }
    }
}
impl Drop for ProofWatch {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn hash(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A cancellation-aware cap also protects against a corrupt compression stream.
struct Output<'a> {
    file: File,
    remaining: u64,
    cancel: &'a AtomicBool,
}
impl Write for Output<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        check(self.cancel).map_err(io::Error::other)?;
        if data.len() as u64 > self.remaining {
            return Err(io::Error::other("Runtime too large"));
        }
        let n = self.file.write(data)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn runtime(base: &Path, cancel: &AtomicBool, progress: &dyn Fn(Event)) -> Result<PathBuf, String> {
    let error = |e: io::Error| e.to_string();
    let directory = base.join("openjdk-25");
    if fs::read_to_string(directory.join(".archive-sha256"))
        .ok()
        .as_deref()
        == Some(RUNTIME_HASH)
        && directory.join("lib/modules").is_file()
        && directory.join("lib/libjli.so").is_file()
        && directory.join("lib/server/libjvm.so").is_file()
    {
        return Ok(directory);
    }
    check(cancel)?;
    let archive = base.join("openjdk-25.tar.xz");
    if hash(&archive).ok().as_deref() != Some(RUNTIME_HASH) {
        let temporary = base.join("openjdk-25.tar.xz.partial");
        let response = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(90))
            .build()
            .get(RUNTIME_URL)
            .call()
            .map_err(|_| "No se pudo descargar OpenJDK. Reintentá la instalación.")?;
        let total = response
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        if total > 96 * 1024 * 1024 {
            return Err("Runtime too large".into());
        }
        let mut response = response.into_reader();
        let mut output = Output {
            file: File::create(&temporary).map_err(error)?,
            remaining: 96 * 1024 * 1024,
            cancel,
        };
        let mut completed = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        progress(Event::Download(official_downloads::Progress {
            stage: "OpenJDK",
            completed,
            total,
            network_bytes: completed,
        }));
        loop {
            check(cancel)?;
            let n = response.read(&mut buffer).map_err(error)?;
            if n == 0 {
                break;
            }
            output.write_all(&buffer[..n]).map_err(error)?;
            completed += n as u64;
            progress(Event::Download(official_downloads::Progress {
                stage: "OpenJDK",
                completed,
                total,
                network_bytes: completed,
            }));
        }
        output.file.sync_all().map_err(error)?;
        if hash(&temporary).map_err(error)? != RUNTIME_HASH {
            let _ = fs::remove_file(&temporary);
            return Err("La descarga de OpenJDK no coincide con su SHA-256 publicado.".into());
        }
        fs::rename(temporary, &archive).map_err(error)?;
    }
    progress(Event::Status(Status::RuntimePreparing));
    let tar_path = base.join("openjdk-25.tar.partial");
    let mut output = Output {
        file: File::create(&tar_path).map_err(error)?,
        remaining: 512 * 1024 * 1024,
        cancel,
    };
    lzma_rs::xz_decompress(
        &mut BufReader::new(File::open(&archive).map_err(error)?),
        &mut output,
    )
    .map_err(|e| format!("No se pudo descomprimir OpenJDK: {e}"))?;
    let staging = base.join("openjdk-25.installing");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(error)?;
    }
    horizon_client::android_runtime_archive::extract_tar(
        File::open(&tar_path).map_err(error)?,
        &staging,
        &|| check(cancel),
    )?;
    fs::write(staging.join("HORIZON-RUNTIME-SOURCE.txt"), format!("OpenJDK 25 Android runtime\nSource/build instructions: {RUNTIME_SOURCE}\nArchive: {RUNTIME_URL}\nSHA-256: {RUNTIME_HASH}\nLicense files are retained in legal/.\n")).map_err(error)?;
    fs::write(staging.join(".archive-sha256"), RUNTIME_HASH).map_err(error)?;
    if directory.exists() {
        fs::remove_dir_all(&directory).map_err(error)?;
    }
    fs::rename(staging, &directory).map_err(error)?;
    let _ = fs::remove_file(tar_path);
    Ok(directory)
}

pub(super) fn prepare(
    internal: &Path,
    helper: &Path,
    cancel: &Arc<AtomicBool>,
    progress: &dyn Fn(Event),
) -> Result<PathBuf, String> {
    check(cancel)?;
    let _proof = ProofWatch::new(cancel.clone());
    // A cancelled HTTP request may still be unwinding when NativeActivity restarts.
    // Never let its cleanup race a new installer using the same verified cache.
    let _installation = loop {
        check(cancel)?;
        match INSTALLER.try_lock() {
            Ok(lock) => break lock,
            Err(TryLockError::Poisoned(error)) => break error.into_inner(),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    let base = internal.join("installation");
    fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let prepared_root = base.join("preparation/.local/horizon-client/26.3");
    let marker = base.join("prepared-complete.json");
    let mut source = Sha256::new();
    source.update(horizon_prepare::SOURCES_SHA256);
    source.update(RUNTIME_HASH);
    for (name, bytes) in PREPARE_SOURCES {
        source.update(name.as_bytes());
        source.update(bytes);
    }
    let key = format!("{}:{:x}", horizon_prepare::SCHEMA, source.finalize());
    if horizon_client::android_prepared_cache::valid(&prepared_root, &marker, &key) {
        check(cancel)?;
        log::info!("Horizon: reusing completed Android installation");
        return Ok(prepared_root);
    }
    let inputs = official_downloads::download(&base.join("minecraft"), cancel, |p| {
        if !auth::android_verified() {
            cancel.store(true, Ordering::Relaxed);
        }
        progress(Event::Download(p));
    })?;
    check(cancel)?;
    let runtime = runtime(&base, cancel, progress)?;
    progress(Event::Status(Status::GamePreparing));
    let root = base.join("preparation");
    for (name, bytes) in PREPARE_SOURCES {
        let target = root.join(name);
        fs::create_dir_all(target.parent().ok_or("Ruta inválida")?).map_err(|e| e.to_string())?;
        fs::write(target, bytes).map_err(|e| e.to_string())?;
    }
    check(cancel)?;
    let jdk =
        horizon_prepare::java::Jdk::android(25, &runtime, helper)?.with_cancel(cancel.clone());
    let options = horizon_prepare::Options {
        root,
        import: true,
        minecraft_dir: Some(inputs.minecraft_dir),
        server_jar: Some(inputs.server_jar),
        java_home: Some(runtime),
    };
    let prepared = horizon_prepare::prepare_with_jdk(&options, jdk, &|line| {
        if !auth::android_verified() {
            cancel.store(true, Ordering::Relaxed);
        }
        log::debug!("Android preparation: {line}");
    })?;
    check(cancel)?;
    progress(Event::Status(Status::Sounds));
    horizon_prepare::sounds(&options.root, options.minecraft_dir.as_deref(), &|s| {
        log::debug!("Android sounds: {s}")
    })?;
    check(cancel)?;
    if let Err(error) = horizon_client::android_prepared_cache::save(&prepared, &marker, &key) {
        log::warn!("Could not cache completed installation: {error}");
    }
    Ok(prepared)
}
