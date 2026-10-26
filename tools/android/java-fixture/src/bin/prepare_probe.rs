//! Developer-only CLI for running the full original exporter pipeline against an
//! explicitly supplied isolated fixture. Installed by Android's package manager,
//! never run from downloaded/writable storage. No accounts or game entry point.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bound developer commands to Android's 120-second device-command window.
/// Cancellation is cooperative through Jdk, which kills and reaps its child.
/// Joining on every return keeps the watchdog from outliving the CLI request.
struct Deadline {
    done: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Deadline {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let stopped = done.clone();
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            while !stopped.load(Ordering::Acquire) {
                if start.elapsed() >= Duration::from_secs(105) {
                    eprintln!("Preparation probe reached its 105-second deadline; cancelling the exporter");
                    cancelled.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        Self {
            done,
            thread: Some(thread),
        }
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn absolute(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err("Every fixture argument must be an absolute path".into());
    }
    path.canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn prepare(args: &[String]) -> Result<(), String> {
    let [root, runtime, helper] = args else {
        return Err(
            "usage: libhorizon_prepare_probe.so FIXTURE_ROOT RUNTIME_HOME INSTALLED_JAVA_HELPER"
                .into(),
        );
    };
    let root = absolute(root)?;
    let runtime = absolute(runtime)?;
    let helper = absolute(helper)?;
    if !helper.starts_with("/data/app")
        || helper.file_name().and_then(|name| name.to_str()) != Some("libhorizon_java.so")
    {
        return Err("Java helper must be the package-manager installed libhorizon_java.so".into());
    }
    for path in [
        "research-lock.json",
        "server.jar",
        "minecraft/versions/26.3/26.3.json",
        "mods/horizon/src/test/java/dev/horizonlab/parity/VanillaHeightmapOracle.java",
    ] {
        if !root.join(path).is_file() {
            return Err(format!("Isolated preparation fixture is missing {path}"));
        }
    }
    // Relative logs/data belong to this explicit developer fixture, never to
    // the caller's working directory or any real player installation.
    std::env::set_current_dir(&root).map_err(|e| format!("Fixture working directory: {e}"))?;
    for name in [
        "HORIZON_HOME",
        "HORIZON_LAUNCHER_HOME",
        "XDG_DATA_HOME",
        "XDG_CONFIG_HOME",
    ] {
        let path = root.join("probe-environment").join(name);
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
        std::env::set_var(name, path);
    }
    std::env::set_var("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1");
    std::env::set_var("HORIZON_NO_AUDIO", "1");
    let options = horizon_prepare::Options {
        root: root.clone(),
        import: true,
        minecraft_dir: Some(root.join("minecraft")),
        server_jar: Some(root.join("server.jar")),
        java_home: None,
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let jdk =
        horizon_prepare::java::Jdk::android(25, &runtime, &helper)?.with_cancel(cancelled.clone());
    let deadline = Deadline::new(cancelled.clone());
    let started = Instant::now();
    let log = |message: &str| println!("[{:.1}s] {message}", started.elapsed().as_secs_f64());
    let result = horizon_prepare::prepare_with_jdk(&options, jdk, &log);
    drop(deadline);
    if cancelled.load(Ordering::Acquire) {
        return Err("Preparation probe stopped at its 105-second deadline; active exporter was cancelled and reaped".into());
    }
    let resources = result?;
    let manifest = resources.join("horizon-resources.json");
    if !manifest.is_file() {
        return Err("Preparation returned without its complete resource manifest".into());
    }
    println!("HORIZON_FULL_ANDROID_PREPARATION_OK {}", manifest.display());
    Ok(())
}

fn main() {
    if let Err(error) = prepare(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("Horizon isolated preparation probe failed: {error}");
        std::process::exit(1);
    }
}
