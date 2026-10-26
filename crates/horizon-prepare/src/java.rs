//! The JDK that compiles and runs the vanilla exporters. They call Mojang's own code, so they
//! stay Java; everything around them is here.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "android")]
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Jdk {
    pub javac: PathBuf,
    pub java: PathBuf,
    #[cfg(target_os = "android")]
    android_helper: Option<PathBuf>,
    #[cfg(target_os = "android")]
    cancelled: Option<Arc<AtomicBool>>,
}

const TIMEOUT: Duration = Duration::from_secs(3600);

/// Exporters and JDK probes write to pipes or log files: on Windows they open no terminal.
fn command(program: impl AsRef<OsStr>) -> Command {
    let command = Command::new(program);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    command
}

fn executable(dir: &Path, name: &str) -> PathBuf {
    dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    })
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| executable(&dir, name))
        .find(|p| p.is_file())
}

/// The major version `javac -version` reports ("javac 25.0.1" → 25; "javac 1.8.0" → 8).
fn javac_major(javac: &Path) -> Result<u32, String> {
    let output = command(javac)
        .arg("-version")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", javac.display()))?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    let version = text
        .split_whitespace()
        .skip_while(|w| *w != "javac")
        .nth(1)
        .ok_or_else(|| {
            // macOS ships /usr/bin/javac as a placeholder that only says no Java is installed.
            if text.contains("Unable to locate a Java Runtime") {
                format!(
                    "{} is macOS's placeholder (no Java installed)",
                    javac.display()
                )
            } else {
                format!(
                    "{} -version printed no version: {}",
                    javac.display(),
                    text.trim()
                )
            }
        })?;
    let mut parts = version.split(['.', '-', '+', '_']);
    let first: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Ok(if first == 1 {
        parts.next().and_then(|p| p.parse().ok()).unwrap_or(0)
    } else {
        first
    })
}

/// The folders JDK installers and tools put JDKs in, each JDK one folder below (#666: a
/// player installed Java 25 but JAVA_HOME and the PATH still named an older one).
fn jdk_parents() -> Vec<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let home = var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let mut parents = Vec::new();
    if cfg!(windows) {
        let vendors = [
            "Eclipse Adoptium",
            "Java",
            "Microsoft",
            "Zulu",
            "BellSoft",
            "Amazon Corretto",
            "Semeru",
            "OpenJDK",
        ];
        for base in ["ProgramFiles", "ProgramW6432"].into_iter().filter_map(var) {
            parents.extend(vendors.iter().map(|v| base.join(v)));
        }
    } else if cfg!(target_os = "macos") {
        parents.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        parents.extend(
            home.iter()
                .map(|h| h.join("Library/Java/JavaVirtualMachines")),
        );
    } else {
        parents.extend(["/usr/lib/jvm", "/usr/java", "/usr/local/lib/jvm"].map(PathBuf::from));
    }
    parents.extend(
        home.iter()
            .flat_map(|h| [h.join(".jdks"), h.join(".sdkman/candidates/java")]),
    );
    parents
}

/// The JDKs (folders with `bin/javac`) one level below `parents`; a macOS bundle's home is
/// its `Contents/Home`. Those named for `major` come first, then the others newest name first.
fn installed_jdks(parents: &[PathBuf], major: i64) -> Vec<PathBuf> {
    let mut homes: Vec<PathBuf> = parents
        .iter()
        .filter_map(|parent| fs::read_dir(parent).ok())
        .flatten()
        .flatten()
        .map(|entry| {
            let bundle = entry.path().join("Contents/Home");
            if bundle.is_dir() {
                bundle
            } else {
                entry.path()
            }
        })
        .filter(|home| executable(&home.join("bin"), "javac").is_file())
        .collect();
    let named = |home: &Path| {
        home.iter().any(|part| {
            part.to_string_lossy()
                .split(|c: char| !c.is_ascii_digit())
                .any(|n| n == major.to_string())
        })
    };
    homes.sort_by(|a, b| named(b).cmp(&named(a)).then_with(|| b.cmp(a)));
    homes.dedup();
    homes
}

/// The Java runtimes the official launcher downloaded into `runtime` (a game directory's
/// `runtime` folder: `<component>/<platform>/<component>/`, a macOS one in `jre.bundle`). Mojang's
/// runtimes are whole JDKs, javac included, so a player who has run Minecraft has one already.
pub fn minecraft_runtimes(runtime: &Path) -> Vec<PathBuf> {
    let children = |dir: &Path| -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        paths.sort();
        paths
    };
    let mut homes = Vec::new();
    for component in children(runtime) {
        for platform in children(&component) {
            let home = platform.join(component.file_name().unwrap_or_default());
            for home in [home.join("jre.bundle/Contents/Home"), home] {
                if executable(&home.join("bin"), "javac").is_file() {
                    homes.push(home);
                    break;
                }
            }
        }
    }
    homes
}

/// Finds a JDK with `javac` of at least `major`, the version research-lock.json pins:
/// `JAVA_HOME` when it holds one, else `javac` and `java` on the PATH (as the Python preparer
/// ran them), else, on macOS, the JDK `/usr/libexec/java_home -v <major>` reports, else the JDK
/// homes in `more` (the launcher's pick, Minecraft's own runtimes), else a JDK in the folders
/// installers put them in (`jdk_parents`).
pub fn find(major: i64, more: &[PathBuf]) -> Result<Jdk, String> {
    let need = format!(
        "Java {major} is needed (a JDK with javac, e.g. Eclipse Temurin {major}). \
         Install it, or set JAVA_HOME to it, and try again."
    );
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME").filter(|h| !h.is_empty()) {
        let bin = Path::new(&home).join("bin");
        candidates.push((
            executable(&bin, "javac"),
            executable(&bin, "java"),
            "JAVA_HOME",
        ));
    }
    if let (Some(javac), Some(java)) = (on_path("javac"), on_path("java")) {
        candidates.push((javac, java, "PATH"));
    }
    if cfg!(target_os = "macos") {
        if let Ok(out) = command("/usr/libexec/java_home")
            .args(["-v", &major.to_string()])
            .stderr(Stdio::null())
            .output()
        {
            let home = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() && !home.is_empty() {
                let bin = Path::new(&home).join("bin");
                candidates.push((
                    executable(&bin, "javac"),
                    executable(&bin, "java"),
                    "/usr/libexec/java_home",
                ));
            }
        }
    }
    for home in more {
        let bin = home.join("bin");
        candidates.push((
            executable(&bin, "javac"),
            executable(&bin, "java"),
            "Minecraft's Java",
        ));
    }
    for home in installed_jdks(&jdk_parents(), major) {
        let bin = home.join("bin");
        candidates.push((
            executable(&bin, "javac"),
            executable(&bin, "java"),
            "an installed JDK",
        ));
    }
    let mut problems = Vec::new();
    for (javac, java, from) in candidates {
        if !javac.is_file() || !java.is_file() {
            problems.push(format!(
                "{from} ({}) has no javac and java",
                javac.parent().unwrap().display()
            ));
            continue;
        }
        match javac_major(&javac) {
            Ok(found) if i64::from(found) >= major => {
                return Ok(Jdk {
                    javac,
                    java,
                    #[cfg(target_os = "android")]
                    android_helper: None,
                    #[cfg(target_os = "android")]
                    cancelled: None,
                })
            }
            Ok(found) => problems.push(format!("{from} has Java {found} ({})", javac.display())),
            Err(e) => problems.push(e),
        }
    }
    if problems.is_empty() {
        Err(format!("{need} No javac was found."))
    } else {
        Err(format!("{need} Found: {}.", problems.join("; ")))
    }
}

/// A classpath with the platform's separator.
pub fn classpath<'a>(paths: impl IntoIterator<Item = &'a Path>) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };
    paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator)
}

impl Jdk {
    /// Use the APK-installed Rust launcher with a separately downloaded OpenJDK.
    /// No writable/downloaded executable is spawned on Android. All environment
    /// changes belong to the child, never to the game's process.
    #[cfg(target_os = "android")]
    pub fn android(major: i64, home: &Path, helper: &Path) -> Result<Self, String> {
        if !home.is_absolute() || !helper.is_absolute() || !helper.is_file() {
            return Err("Android Java runtime and installed helper must be absolute paths".into());
        }
        if !home.join("lib/modules").is_file() || !home.join("lib/libjli.so").is_file() {
            return Err("Android OpenJDK installation is incomplete".into());
        }
        let jdk = Self {
            javac: home.join("bin/javac"),
            java: home.join("bin/java"),
            android_helper: Some(helper.to_path_buf()),
            cancelled: None,
        };
        let output = jdk
            .launch_command(true)
            .arg("-version")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("Cannot start installed Java preparation helper: {e}"))?;
        let version = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        let actual = version
            .split_whitespace()
            .skip_while(|part| *part != "javac")
            .nth(1)
            .and_then(|v| v.split('.').next())
            .and_then(|v| v.parse::<i64>().ok());
        if !output.status.success() || actual.is_none_or(|found| found < major) {
            return Err(format!(
                "Java preparation requires javac {major}; helper probe failed: {}",
                version.trim()
            ));
        }
        Ok(jdk)
    }

    /// Cancel and reap the current exporter promptly instead of leaving a JVM
    /// behind when setup is cancelled or the authenticated session expires.
    #[cfg(target_os = "android")]
    pub fn with_cancel(mut self, cancelled: Arc<AtomicBool>) -> Self {
        self.cancelled = Some(cancelled);
        self
    }

    fn cancellation(&self) -> Option<&AtomicBool> {
        #[cfg(target_os = "android")]
        return self.cancelled.as_deref();
        #[cfg(not(target_os = "android"))]
        None
    }

    fn launch_command(&self, compiler: bool) -> Command {
        #[cfg(target_os = "android")]
        if let Some(helper) = &self.android_helper {
            let home = self.home();
            let lib = home.join("lib");
            let native = helper.parent().unwrap_or_else(|| Path::new("/"));
            // OpenJDK requires these exact leading entries or tries to re-exec
            // its java executable, which Android must never execute from writable storage.
            let paths = [lib.join("server"), lib.clone(), native.to_path_buf()];
            let mut command = command(helper);
            command
                .arg(&home)
                .arg(if compiler { "javac" } else { "java" })
                .env("JAVA_HOME", &home)
                .env("LD_LIBRARY_PATH", std::env::join_paths(paths).unwrap())
                .env_remove("JAVA_TOOL_OPTIONS")
                .env_remove("_JAVA_OPTIONS")
                .env_remove("JDK_JAVA_OPTIONS");
            return command;
        }
        command(if compiler { &self.javac } else { &self.java })
    }

    /// The JDK's home, the folder above `bin` (what `JAVA_HOME` names).
    pub fn home(&self) -> PathBuf {
        self.javac
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }

    /// `javac --release <major> -proc:none` with Python's `subprocess.run(..., check=True,
    /// stdout=log, stderr=STDOUT, timeout=3600)`.
    pub fn javac(&self, major: i64, args: &[String], log: &fs::File) -> Result<(), String> {
        let mut command = self.launch_command(true);
        command
            .args(["--release", &major.to_string(), "-proc:none"])
            .args(args);
        run(command, log, self.cancellation())
    }

    pub fn java(&self, args: &[String], cwd: Option<&Path>, log: &fs::File) -> Result<(), String> {
        let mut command = self.launch_command(false);
        command.args(args);
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }
        run(command, log, self.cancellation())
    }
}

fn run(mut command: Command, log: &fs::File, cancelled: Option<&AtomicBool>) -> Result<(), String> {
    let is_cancelled = || cancelled.is_some_and(|flag| flag.load(Ordering::Acquire));
    if is_cancelled() {
        return Err("Resource preparation cancelled".into());
    }
    let out = log.try_clone().map_err(|e| e.to_string())?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let program = Path::new(command.get_program())
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Cannot start {program}: {e}"))?;
    let start = Instant::now();
    loop {
        if is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Resource preparation cancelled".into());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err(format!("{program} failed ({status})"))
            };
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{program} took over an hour and was stopped"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn cancellation_stops_and_reaps_a_running_exporter() {
        let log_path =
            std::env::temp_dir().join(format!("horizon-prepare-cancel-{}.log", std::process::id()));
        let log = fs::File::create(&log_path).unwrap();
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            signal.store(true, Ordering::Release);
        });
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let started = Instant::now();
        let result = run(command, &log, Some(&cancelled));
        thread.join().unwrap();
        assert_eq!(result.unwrap_err(), "Resource preparation cancelled");
        assert!(started.elapsed() < Duration::from_secs(5));
        // A pre-cancelled request must not even try to spawn its executable.
        let result = run(
            Command::new("/nonexistent-horizon-exporter"),
            &log,
            Some(&cancelled),
        );
        assert_eq!(result.unwrap_err(), "Resource preparation cancelled");
        drop(log);
        fs::remove_file(log_path).unwrap();
    }

    #[test]
    fn installed_jdks_prefer_the_pinned_major() {
        let dir = std::env::temp_dir().join(format!("horizon-prepare-jdks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let parent = dir.join("Eclipse Adoptium");
        for jdk in [
            "jdk-21.0.5.11-hotspot",
            "jdk-25.0.1.8-hotspot",
            "jdk-26.0.0-hotspot",
            "jre-25.0.1",
        ] {
            let bin = parent.join(jdk).join("bin");
            fs::create_dir_all(&bin).unwrap();
            if jdk.starts_with("jdk") {
                fs::write(executable(&bin, "javac"), b"").unwrap();
            }
        }
        // A macOS bundle keeps its home in Contents/Home.
        let bundle = dir.join("JavaVirtualMachines/temurin-25.jdk/Contents/Home");
        fs::create_dir_all(bundle.join("bin")).unwrap();
        fs::write(executable(&bundle.join("bin"), "javac"), b"").unwrap();
        let found = installed_jdks(
            &[
                parent.clone(),
                dir.join("JavaVirtualMachines"),
                dir.join("missing"),
            ],
            25,
        );
        assert_eq!(
            found,
            [
                bundle,
                parent.join("jdk-25.0.1.8-hotspot"),
                parent.join("jdk-26.0.0-hotspot"),
                parent.join("jdk-21.0.5.11-hotspot"),
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn minecraft_runtimes_are_found_in_the_launchers_layout() {
        let dir =
            std::env::temp_dir().join(format!("horizon-prepare-runtimes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let make = |home: PathBuf, javac: bool| {
            fs::create_dir_all(home.join("bin")).unwrap();
            if javac {
                fs::write(executable(&home.join("bin"), "javac"), b"").unwrap();
            }
            home
        };
        let gamma = make(
            dir.join("java-runtime-gamma/linux/java-runtime-gamma"),
            true,
        );
        let epsilon = make(
            dir.join(
                "java-runtime-epsilon/mac-os-arm64/java-runtime-epsilon/jre.bundle/Contents/Home",
            ),
            true,
        );
        make(dir.join("jre-legacy/linux/jre-legacy"), false);
        assert_eq!(minecraft_runtimes(&dir), [epsilon, gamma]);
        assert!(minecraft_runtimes(&dir.join("missing")).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
