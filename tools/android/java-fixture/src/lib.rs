mod service;
// Isolated development fixture: exercises the APK-installed helper in a target35
// app sandbox. No Minecraft classes, resources, accounts or saves are accessed.
use android_activity::{AndroidApp, MainEvent, PollEvent};
use jni::objects::{JObject, JString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn native_dir(app: &AndroidApp) -> Result<PathBuf, String> {
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    env.with_local_frame(8, |env| -> jni::errors::Result<PathBuf> {
        let info = env
            .call_method(
                &activity,
                "getApplicationInfo",
                "()Landroid/content/pm/ApplicationInfo;",
                &[],
            )?
            .l()?;
        let directory = env
            .get_field(info, "nativeLibraryDir", "Ljava/lang/String;")?
            .l()?;
        let value: String = env.get_string(&JString::from(directory))?.into();
        Ok(value.into())
    })
    .map_err(|e| e.to_string())
}

fn copy_runtime(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Create {}: {e}", to.display()))?;
    for entry in
        fs::read_dir(from).map_err(|e| format!("Read runtime directory {}: {e}", from.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = entry.file_type().map_err(|e| e.to_string())?;
        let destination = to.join(entry.file_name());
        if metadata.is_dir() {
            copy_runtime(&entry.path(), &destination)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), &destination).map_err(|e| {
                format!(
                    "Copy {} to {}: {e}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        } else {
            return Err("Fixture input must contain regular files/directories only".into());
        }
    }
    Ok(())
}

fn probe(app: &AndroidApp) -> Result<(), String> {
    let external = app
        .external_data_path()
        .ok_or("No fixture external directory")?;
    let internal = app
        .internal_data_path()
        .ok_or("No fixture internal directory")?;
    fs::create_dir_all(&external).map_err(|e| e.to_string())?;
    let log_path = external.join("probe.log");
    match fs::remove_file(external.join("probe-error.txt")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Clear previous fixture error: {error}")),
    }
    fs::write(&log_path, b"").map_err(|e| e.to_string())?;
    let mut log = fs::OpenOptions::new()
        .append(true)
        .open(&log_path)
        .map_err(|e| e.to_string())?;
    use std::io::Write;
    writeln!(
        log,
        "START targetSdk35 app-sandbox Java preparation fixture"
    )
    .map_err(|e| e.to_string())?;
    let home = internal.join("runtime");
    if !home.join("lib/modules").is_file() {
        copy_runtime(&external.join("runtime"), &home)?;
    }
    let native = native_dir(app)?;
    let helper = native.join("libhorizon_java.so");
    writeln!(log, "Installed helper: {}", helper.display()).map_err(|e| e.to_string())?;
    let run = |mode: &str, args: &[&str], expected_success: bool| -> Result<(), String> {
        let output = fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .map_err(|e| e.to_string())?;
        let mut child = Command::new(&helper)
            .arg(&home)
            .arg(mode)
            .args(args)
            .env("JAVA_HOME", &home)
            .env(
                "LD_LIBRARY_PATH",
                std::env::join_paths([home.join("lib/server"), home.join("lib"), native.clone()])
                    .unwrap(),
            )
            .env_remove("JAVA_TOOL_OPTIONS")
            .env_remove("_JAVA_OPTIONS")
            .env_remove("JDK_JAVA_OPTIONS")
            .stdin(Stdio::null())
            .stdout(output.try_clone().map_err(|e| e.to_string())?)
            .stderr(output)
            .spawn()
            .map_err(|e| format!("{mode}: {e}"))?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return if status.success() == expected_success {
                    Ok(())
                } else {
                    Err(format!("{mode} exited unexpectedly: {status}"))
                };
            }
            if started.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{mode} timed out"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    run("java", &["-version"], true)?;
    run("javac", &["-version"], true)?;
    let source = internal.join("PreparationProbe.java");
    fs::write(&source, r#"public final class PreparationProbe {
        public static void main(String[] args) {
            if (Runtime.version().feature() != 25) throw new AssertionError("Java 25 needed");
            if (!"true".equals(System.getProperty("java.awt.headless"))) throw new AssertionError("headless missing");
            if (args.length != 1 || !args[0].equals("argument with spaces")) throw new AssertionError("arguments lost");
            System.out.println("HORIZON_PREPARATION_JVM_OK");
        }
    }"#).map_err(|e| e.to_string())?;
    let source_string = source.to_string_lossy();
    let classes = internal.to_string_lossy();
    run(
        "javac",
        &[
            "--release",
            "25",
            "-proc:none",
            "-d",
            &classes,
            &source_string,
        ],
        true,
    )?;
    run(
        "java",
        &["-cp", &classes, "PreparationProbe", "argument with spaces"],
        true,
    )?;
    run("java", &["-cp", &classes, "MissingClass"], false)?;
    writeln!(
        log,
        "PASS app-sandbox helper, javac25 compile, java25 run, argument preservation, error exit"
    )
    .map_err(|e| e.to_string())?;
    let preparation = external.join("preparation");
    if preparation.join("research-lock.json").is_file() {
        writeln!(log, "START isolated complete original exporter preparation")
            .map_err(|e| e.to_string())?;
        let jdk = horizon_prepare::java::Jdk::android(25, &home, &helper)?;
        let options = horizon_prepare::Options {
            root: preparation.clone(),
            import: true,
            minecraft_dir: Some(preparation.join("minecraft")),
            server_jar: Some(preparation.join("server.jar")),
            java_home: None,
        };
        let progress = |message: &str| {
            if let Ok(mut file) = fs::OpenOptions::new().append(true).open(&log_path) {
                let _ = writeln!(file, "{message}");
            }
        };
        let resources = horizon_prepare::prepare_with_jdk(&options, jdk, &progress)?;
        let manifest = resources.join("horizon-resources.json");
        if !manifest.is_file() {
            return Err("Full preparation did not produce its manifest".into());
        }
        writeln!(
            log,
            "PASS complete original exporter preparation: {}",
            manifest.display()
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let done = Arc::new(AtomicBool::new(false));
    let mut started = false;
    let mut destroyed = false;
    while !destroyed && !done.load(Ordering::Acquire) {
        app.poll_events(Some(Duration::from_millis(100)), |event| {
            if matches!(event, PollEvent::Main(MainEvent::Resume { .. })) && !started {
                started = true;
                let app = app.clone();
                let done = done.clone();
                std::thread::spawn(move || {
                    if let Err(error) = probe(&app) {
                        if let Some(dir) = app.external_data_path() {
                            let _ = fs::create_dir_all(&dir);
                            let _ = fs::write(dir.join("probe-error.txt"), error);
                        }
                    }
                    done.store(true, Ordering::Release);
                });
            }
            if matches!(event, PollEvent::Main(MainEvent::Destroy)) {
                destroyed = true;
            }
        });
    }
}
