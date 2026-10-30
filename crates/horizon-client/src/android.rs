//! Android NativeActivity entry point. All rendering and gameplay use the desktop engine.
#[cfg(target_os = "android")]
mod game;

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;
#[cfg(target_os = "android")]
thread_local! {
    static ANDROID_APP: std::cell::RefCell<Option<AndroidApp>> = const { std::cell::RefCell::new(None) };
}
#[cfg(target_os = "android")]
pub(crate) fn android_app() -> AndroidApp {
    ANDROID_APP.with(|app| app.borrow().clone().expect("Android entry point"))
}

#[cfg(target_os = "android")]
static SERVER_DNS: std::sync::OnceLock<(
    jni::JavaVM,
    jni::objects::GlobalRef,
    jni::objects::GlobalRef,
)> = std::sync::OnceLock::new();
#[cfg(target_os = "android")]
fn initialize_server_dns() -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    let app = android_app();
    // SAFETY: AndroidApp owns the live VM and Activity.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
    let (context, class) = {
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let result = env.with_local_frame(12, |env| -> jni::errors::Result<_> {
            let context = env
                .call_method(
                    &activity,
                    "getApplicationContext",
                    "()Landroid/content/Context;",
                    &[],
                )?
                .l()?;
            let loader = env
                .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
                .l()?;
            let name = JObject::from(env.new_string("dev.horizon.android.ServerDns")?);
            let class = env
                .call_method(
                    loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)],
                )?
                .l()?;
            Ok((env.new_global_ref(context)?, env.new_global_ref(class)?))
        });
        if result.is_err() {
            let _ = env.exception_clear();
        }
        result.map_err(|e| e.to_string())?
    };
    let _ = SERVER_DNS.set((vm, context, class));
    horizon_protocol::set_android_dns_resolver(|name| {
        let (vm, context, class) = SERVER_DNS.get()?;
        let mut env = vm.attach_current_thread().ok()?;
        let result = env.with_local_frame(8, |env| -> jni::errors::Result<Option<Vec<u8>>> {
            let name = JObject::from(env.new_string(name)?);
            let class = <&jni::objects::JClass>::from(class.as_obj());
            let result = env
                .call_static_method(
                    class,
                    "query",
                    "(Landroid/content/Context;Ljava/lang/String;)[B",
                    &[JValue::Object(context.as_obj()), JValue::Object(&name)],
                )?
                .l()?;
            if result.is_null() {
                return Ok(None);
            }
            Ok(Some(env.convert_byte_array(
                jni::objects::JByteArray::from(result),
            )?))
        });
        if result.is_err() {
            let _ = env.exception_clear();
        }
        result.ok().flatten()
    });
    Ok(())
}

/// Finish the startup task instead of reopening the mandatory account panel.
#[cfg(target_os = "android")]
pub(crate) fn android_finish() -> Result<(), String> {
    let app = android_app();
    // SAFETY: AndroidApp owns the live VM and Activity for this call.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { jni::objects::JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = env.call_method(&activity, "finishAndRemoveTask", "()V", &[]);
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Own the foreground service for exactly one authenticated installation. Started while visible;
/// stopped from the worker even when the Activity's event loop is suspended.
#[cfg(target_os = "android")]
static INSTALL_CANCEL: std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> =
    std::sync::Mutex::new(None);

#[cfg(target_os = "android")]
pub(crate) struct AndroidInstallation {
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    vm: jni::JavaVM,
    context: jni::objects::GlobalRef,
    intent: jni::objects::GlobalRef,
}
#[cfg(target_os = "android")]
impl AndroidInstallation {
    pub(crate) fn start(
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        use jni::objects::{JObject, JValue};
        // Serialize service commands as well as ownership: an old worker may still unwind
        // after NativeActivity has created a replacement installer.
        let mut owner = INSTALL_CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        let app = android_app();
        // SAFETY: AndroidApp owns the live VM and Activity during this call.
        let vm =
            unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
        let result = {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
            let result = env.with_local_frame(12, |env| -> jni::errors::Result<_> {
                let context = env
                    .call_method(
                        &activity,
                        "getApplicationContext",
                        "()Landroid/content/Context;",
                        &[],
                    )?
                    .l()?;
                let intent = env.new_object("android/content/Intent", "()V", &[])?;
                let package = JObject::from(env.new_string("dev.horizon.android")?);
                let name =
                    JObject::from(env.new_string("dev.horizon.android.InstallationService")?);
                env.call_method(
                    &intent,
                    "setClassName",
                    "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
                    &[JValue::Object(&package), JValue::Object(&name)],
                )?;
                env.call_method(
                    &context,
                    "startForegroundService",
                    "(Landroid/content/Intent;)Landroid/content/ComponentName;",
                    &[JValue::Object(&intent)],
                )?;
                Ok((env.new_global_ref(context)?, env.new_global_ref(intent)?))
            });
            if result.is_err() {
                let _ = env.exception_clear();
            }
            result
        };
        let (context, intent) =
            result.map_err(|e| format!("Could not start installation service: {e}"))?;
        *owner = Some(cancel.clone());
        Ok(Self {
            cancel,
            vm,
            context,
            intent,
        })
    }
}
#[cfg(target_os = "android")]
impl Drop for AndroidInstallation {
    fn drop(&mut self) {
        let mut owner = INSTALL_CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        if !owner
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &self.cancel))
        {
            return;
        }
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let result = env.call_method(
                self.context.as_obj(),
                "stopService",
                "(Landroid/content/Intent;)Z",
                &[jni::objects::JValue::Object(self.intent.as_obj())],
            );
            if result.is_err() {
                let _ = env.exception_clear();
            }
        }
        *owner = None;
    }
}
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_horizon_android_InstallationService_cancelInstallation(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
) {
    if let Some(cancel) = &*INSTALL_CANCEL.lock().unwrap_or_else(|e| e.into_inner()) {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Read Android's locale only for a first launch without a saved Minecraft language.
#[cfg(target_os = "android")]
pub(crate) fn android_locale() -> Option<String> {
    let app = android_app();
    // SAFETY: the VM is owned by the AndroidApp stored for this native event loop.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let result = env.with_local_frame(8, |env| -> jni::errors::Result<String> {
        let locale = env
            .call_static_method(
                "java/util/Locale",
                "getDefault",
                "()Ljava/util/Locale;",
                &[],
            )?
            .l()?;
        let tag = env
            .call_method(locale, "toLanguageTag", "()Ljava/lang/String;", &[])?
            .l()?;
        let tag = jni::objects::JString::from(tag);
        Ok(env.get_string(&tag)?.into())
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.ok()
}

#[cfg(target_os = "android")]
fn initialize_language(home: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    let path = home.join("options.txt");
    let contents = std::fs::read_to_string(&path)?;
    if contents.lines().any(|line| {
        line.strip_prefix("lang:")
            .is_some_and(|v| !v.trim().is_empty())
    }) {
        return Ok(());
    }
    let locale = android_locale().unwrap_or_else(|| "en-US".into());
    let code = game::mobile_locale::minecraft_locale(&locale);
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    if !contents.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "lang:{code}")
}

/// Executable preparation helpers are installed by Android, never executed from downloads.
#[cfg(target_os = "android")]
pub(crate) fn android_native_library_dir() -> Result<std::path::PathBuf, String> {
    use jni::objects::{JObject, JString};
    let app = android_app();
    // SAFETY: the Activity and VM remain owned by AndroidApp throughout this call.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|_| "No se pudo acceder al directorio de instalación.".to_owned())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "No se pudo acceder a Android.".to_owned())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = env.with_local_frame(8, |env| -> jni::errors::Result<std::path::PathBuf> {
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
        let directory = JString::from(directory);
        let value: String = env.get_string(&directory)?.into();
        Ok(value.into())
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map_err(|_| "Android no devolvió el directorio de bibliotecas.".into())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let result = (|| -> horizon_client::Result<()> {
        let home = app
            .external_data_path()
            .or_else(|| app.internal_data_path())
            .ok_or("Android data directory unavailable")?;
        std::fs::create_dir_all(&home)?;
        std::env::set_current_dir(&home)?;
        // NativeActivity has no command line: resources/ lives beside saves/ and options.txt.
        if !home.join("options.txt").exists() {
            std::fs::write(
                home.join("options.txt"),
                "renderDistance:6\nsimulationDistance:5\nmaxFps:60\nenableVsync:true\ngraphicsMode:0\n",
            )?;
        }
        ANDROID_APP.with(|a| *a.borrow_mut() = Some(app));
        if let Err(error) = initialize_server_dns() {
            log::warn!("Android DNS bridge: {error}");
        }
        initialize_language(&home)?;
        horizon_client::logging::init(&home, "Render thread");
        horizon_client::crash::install(home.clone(), "client");
        game::run()
    })();
    if let Err(error) = result {
        log::error!("Horizon Android: {error}");
    }
    horizon_client::logging::flush();
}

/// Android does not send a final axis/button release on every Bluetooth disconnect.
#[cfg(target_os = "android")]
pub(crate) fn android_controller_exists(id: i32) -> bool {
    {
        let app = android_app();
        // SAFETY: AndroidApp keeps the VM alive throughout this native event loop.
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }) else {
            return true;
        };
        let Ok(mut env) = vm.get_env() else {
            return true;
        };
        let result = env.with_local_frame(4, |env| -> jni::errors::Result<bool> {
            let device = env
                .call_static_method(
                    "android/view/InputDevice",
                    "getDevice",
                    "(I)Landroid/view/InputDevice;",
                    &[jni::objects::JValue::Int(id)],
                )?
                .l()?;
            Ok(!device.is_null())
        });
        if result.is_err() {
            let _ = env.exception_clear();
        }
        result.unwrap_or(true)
    }
}

/// Open Microsoft's public verification page in the user's system browser. Credentials and tokens
/// never enter an embedded web view or the URI. Only a user press in the account panel calls this.
#[cfg(target_os = "android")]
pub(crate) fn android_open_browser(url: &str) -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    if !matches!(
        url,
        "https://www.microsoft.com/link"
            | "https://microsoft.com/link"
            | "https://www.microsoft.com/devicelogin"
            | "https://microsoft.com/devicelogin"
            | "https://login.microsoftonline.com/common/oauth2/deviceauth"
    ) {
        return Err("La dirección de inicio de sesión no pertenece a Microsoft.".into());
    }
    let app = android_app();
    // SAFETY: AndroidApp owns a live VM and Activity for this event loop's lifetime. The borrowed
    // Activity reference is never released or stored by this helper.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|_| "No se pudo conectar con el navegador de Android.".to_owned())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "No se pudo abrir el navegador de Android.".to_owned())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = env.with_local_frame(12, |env| -> jni::errors::Result<()> {
        let uri_text = JObject::from(env.new_string(url)?);
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&uri_text)],
            )?
            .l()?;
        let action = JObject::from(env.new_string("android.intent.action.VIEW")?);
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(
            &activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result
        .map_err(|_| "No se pudo abrir Microsoft. Abrí microsoft.com/link en tu navegador.".into())
}

/// Copy only Microsoft's short linking code after an explicit sign-in press.
#[cfg(target_os = "android")]
pub(crate) fn android_copy_link_code(code: &str) -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    if code.is_empty()
        || code.len() > 32
        || !code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Código de vinculación inválido.".into());
    }
    let app = android_app();
    // SAFETY: the VM and borrowed Activity are owned by the live AndroidApp.
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|_| "No se pudo acceder al portapapeles.".to_owned())?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "No se pudo acceder al portapapeles.".to_owned())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let result = env.with_local_frame(12, |env| -> jni::errors::Result<()> {
        let service_name = JObject::from(env.new_string("clipboard")?);
        let clipboard = env
            .call_method(
                &activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service_name)],
            )?
            .l()?;
        let label = JObject::from(env.new_string("Código de Microsoft")?);
        let text = JObject::from(env.new_string(code)?);
        let clip = env
            .call_static_method(
                "android/content/ClipData",
                "newPlainText",
                "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                &[JValue::Object(&label), JValue::Object(&text)],
            )?
            .l()?;
        env.call_method(
            &clipboard,
            "setPrimaryClip",
            "(Landroid/content/ClipData;)V",
            &[JValue::Object(&clip)],
        )?;
        Ok(())
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map_err(|_| "No se pudo copiar el código.".into())
}
