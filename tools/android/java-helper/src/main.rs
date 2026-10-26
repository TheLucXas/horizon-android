//! One short-lived OpenJDK process per preparation exporter. Android's package manager
//! installs this Rust executable as libhorizon_java.so; downloaded Java binaries are
//! never executed. The game itself remains Rust and does not load a JVM.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

// OpenJDK's JLI_Launch entry point (src/java.base/share/native/libjli/java.h).
// This ABI is pinned together with the downloaded OpenJDK runtime; it is not JNI.
type Launch = unsafe extern "C" fn(
    c_int,
    *mut *mut c_char,
    c_int,
    *const *const c_char,
    c_int,
    *const *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    u8,
    u8,
    u8,
    c_int,
) -> c_int;

fn loader_error() -> String {
    // SAFETY: dlerror returns a null pointer or a loader-owned NUL-terminated string.
    unsafe {
        let error = dlerror();
        if error.is_null() {
            "unknown dynamic-loader error".into()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    }
}

fn launch(args: Vec<String>) -> Result<i32, String> {
    let [home, mode, java_args @ ..] = args.as_slice() else {
        return Err("usage: libhorizon_java.so RUNTIME_HOME java|javac [arguments]".into());
    };
    if !matches!(mode.as_str(), "java" | "javac") {
        return Err("helper mode must be java or javac".into());
    }
    let home = Path::new(home);
    if !home.is_absolute() || !home.join("lib/modules").is_file() {
        return Err("runtime home must be absolute and contain lib/modules".into());
    }
    let temp = home
        .parent()
        .ok_or("Runtime home has no parent")?
        .join("java-tmp");
    std::fs::create_dir_all(&temp)
        .map_err(|e| format!("Cannot create Java preparation temporary directory: {e}"))?;
    let mut argv = vec![home.join("bin/java").to_string_lossy().into_owned()];
    argv.extend([
        format!("-Djava.home={}", home.display()),
        "-Djava.awt.headless=true".into(),
        format!("-Djava.io.tmpdir={}", temp.display()),
        "-XX:-UsePerfData".into(),
    ]);
    if mode == "javac" {
        argv.extend([
            "--module".into(),
            "jdk.compiler/com.sun.tools.javac.Main".into(),
        ]);
    }
    argv.extend_from_slice(java_args);
    let mut strings: Vec<CString> = argv
        .into_iter()
        .map(|arg| CString::new(arg).map_err(|_| "NUL in Java argument".to_string()))
        .collect::<Result<_, _>>()?;
    let mut pointers: Vec<*mut c_char> = strings
        .iter_mut()
        .map(|arg| arg.as_ptr().cast_mut())
        .collect();
    let argc = c_int::try_from(pointers.len()).map_err(|_| "too many arguments")?;
    pointers.push(std::ptr::null_mut());
    let library = CString::new(home.join("lib/libjli.so").as_os_str().as_encoded_bytes())
        .map_err(|_| "NUL in runtime path")?;
    // SAFETY: this child loads only the application's hash-verified, pinned runtime;
    // all C strings and argv storage outlive the call. RTLD_NOW fails on missing
    // dependencies before invoking the ABI. Keep the library open until process exit.
    let code = unsafe {
        let handle = dlopen(library.as_ptr(), 2); // RTLD_NOW
        if handle.is_null() {
            return Err(format!("Cannot load OpenJDK launcher: {}", loader_error()));
        }
        let symbol = dlsym(handle, c"JLI_Launch".as_ptr());
        if symbol.is_null() {
            return Err(format!("Cannot find OpenJDK launcher: {}", loader_error()));
        }
        let jli: Launch = std::mem::transmute(symbol);
        jli(
            argc,
            pointers.as_mut_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            c"25".as_ptr(),
            c"25".as_ptr(),
            c"java".as_ptr(),
            c"java".as_ptr(),
            0,
            1,
            0,
            0,
        )
    };
    Ok(code)
}

fn main() {
    let args = std::env::args().skip(1).collect();
    match launch(args) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("Horizon preparation JVM: {error}");
            std::process::exit(1);
        }
    }
}
