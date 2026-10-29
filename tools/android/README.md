# Horizon Android (development)

The Android target shares the original Rust game, simulation and wgpu Vulkan renderer.
It uses NativeActivity with no Java game wrapper. API 28+, ARM64 and a suitable Vulkan
GPU are required. Performance and hardware compatibility must be tested on each device.

## Build on Linux/macOS with the Android SDK and NDK

Install Rust 1.90+, the `aarch64-linux-android` target and `cargo-ndk`. Then:

```sh
cargo ndk --target arm64-v8a --platform 28 build -p horizon-android --release
keytool -genkeypair -keystore android-debug.keystore -storepass android -keypass android \
  -alias androiddebugkey -dname 'CN=Horizon Android Development' -keyalg RSA -validity 10000
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android28-clang" \
  cargo build --manifest-path tools/android/java-helper/Cargo.toml --target aarch64-linux-android --release --locked
python3 tools/android/package.py --sdk "$ANDROID_HOME" --keystore android-debug.keystore \
  --java-helper tools/android/java-helper/target/aarch64-linux-android/release/horizon-android-java-helper \
  --cxx-runtime "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so"
```

Keep this keystore private and reuse it for local updates. This is a development signing
key, not a publisher's release identity. The APK contains no Mojang resources.

## First-run installation

The Android startup uses only original geometric widgets and the bundled OFL-licensed
Pixelify Sans bitmap font until the account has been verified. Microsoft sign-in, Java Edition
entitlements and the matching official profile are mandatory before downloading
Minecraft inputs or entering any local world or server. Cached accounts are checked
online on startup; failed verification does not fall back to a guest identity.
This policy is specific to this Android port. Desktop behavior is unchanged.

The installer downloads the pinned 26.3 version JSON, client/server JARs, portable
Java libraries and indexed assets from official Mojang endpoints. Each file is checked
against its published size and SHA-1 before atomic cache publication. It then downloads
OpenJDK25 for Android **separately**, checks the pinned SHA-256, preserves its license
files and runs the original preparation tools locally. The APK includes a small Rust
JVM launcher and NDK libc++, with their notices; it contains neither OpenJDK's runtime
nor Mojang JARs, textures, sounds or prepared tables. See
[the preparation architecture and validation limits](../../docs/android-resource-preparation.md).

Installation inputs, generated tables and account tokens remain in the application's
private internal storage. Worlds, options and touch settings retain their existing
Android external app-data location. A failed/cancelled setup keeps verified downloads
for retry and never enters the game with an incomplete installation. Expired or revoked
ownership proof stops preparation and blocks gameplay; leaving a world uses the normal
save/disconnect path.

The application ID has received Mojang approval, and real sign-in has reached installation.
End-to-end installation, gameplay and premium-server compatibility remain under validation.

## Controls

Two sticks move/look; buttons jump, sneak, mine, use, open inventory and pause. Bumpers
cycle hotbar slots. Touch menus directly. Tap the small pencil during play, drag any control, adjust
size/opacity, then LISTO to save. Circular icon buttons show press feedback; the
look stick appears while touched or when editing. Advanced settings are in `horizon-touch.json`, including
look sensitivity and automatic hiding with a controller. Touch the screen to show controls
again. Existing controller bindings/deadzones/menu settings are shared with desktop Horizon.

The vendored winit 0.30.13 has a small Android-only input patch in
`src/platform_impl/android/mod.rs`: joystick axes and pad keys become DeviceEvents instead
of touches. Upstream winit's license is retained in `third_party/winit/LICENSE`.

## Keyboard and Microsoft account

Android opens the software keyboard when an editable field receives focus, hides it
when leaving the field/screen, and can reopen it after Back by tapping the field.
NativeActivity's basic input path still needs validation with each installed IME;
advanced composition is not claimed as universally supported.

The title's Minecraft-style **Sign in / Iniciar sesión** button follows the selected
Minecraft language and uses the port owner's registered public
Microsoft application ID, configured internally. Players do not enter GUIDs or
register individual Azure apps. Pressing sign-in requests a linking code, copies
that short code to Android's clipboard when possible, and opens Microsoft's official
verification page in the system browser. Complete the page and return to Horizon.
The page may require pasting the code before entering the Microsoft account.
The pending request survives temporary connection failures and pauses network polling
while the app is inactive; returning resumes it until the linking code expires.

Credentials are entered only on Microsoft. Tokens remain in private internal
storage. Minecraft may require developer app approval independently of enabling
public client flows in Entra. See [Microsoft account setup](../../docs/microsoft-account.md).
Real sign-in and skin rendering require an account and cannot be verified by mocked
backend tests alone.

### Original first-run interface (0.5.1)

The pre-download account panel uses a locally generated Pixelify Sans bitmap atlas
and geometric widgets, retaining the font's OFL license. It does not require any
Minecraft resource. Whole-pixel glyph metrics and binary coverage avoid scaling
antialiased TTF text into a blurred first-run screen. The saved Minecraft language
has priority; only a first launch without `lang:` reads Android's default locale.
The bootstrap panel includes Spanish, English and Portuguese strings, with English
fallback for other languages. Once resources are prepared the game continues to use
its normal language configuration. The mandatory sign-in panel has no previous menu;
its close action finishes the Android task instead of reopening the same panel.

### Manual GitHub builds

The `Android APK` workflow runs only through `workflow_dispatch`; pushes do not
start an Android build. Use GitHub Actions → Android APK → Run workflow, selecting
the `android` branch, or `gh workflow run android.yml --ref android`.
Successful runs provide the `horizon-android-arm64` APK artifact.
The workflow uses a standard `ubuntu-latest` GitHub-hosted runner in this public repository.

### Android private-directory aliases (0.5.2)

NativeActivity may return `/data/user/0/...`, where Android links `user/0` to
`/data/data`. Setup canonicalizes this trusted OS-provided app directory before
constructing installation paths. Symlinks in the installation/cache descendants
remain forbidden; no downloaded or archive-controlled path is canonicalized to
bypass those checks. The APK launcher icon reuses upstream Horizon artwork under
its MIT license, with attribution in CREDITS.md.

### Download telemetry and installer language (0.5.3)

The installer reads Android's locale directly, independently of a saved Minecraft
language, with Spanish/English/Portuguese strings and English fallback. Each
file group is planned before downloading, keeping its byte total fixed; duplicate
asset hashes share one cache object. Progress counts verified cache hits, but speed
counts only bytes received from the network, including retransmissions. MB/GB use
decimal units (1 GB = 1,000 MB). A rolling throughput estimate supplies the ETA for
the current download group, with calculating/unknown states during warm-up,
stalls or missing Content-Length. Preparation/extraction stages have no download
speed or invented ETA. Verified cached files are reused on retry/update.

### Concurrent and background installation (0.5.4)

Indexed resource objects now download with up to eight concurrent requests through one
shared HTTP agent. The stage total stays fixed; progress sums received bytes across workers
and excludes cache hits from speed. Each object still needs the declared size and SHA-1
before publication. Transport failures retry with bounded backoff; failed integrity checks
retain the shorter retry limit. Completed verified files are reused after interruption.

A small Android framework Service bridge (compiled with the SDK, without AndroidX) owns
a `dataSync` foreground notification and a partial wake lock during the authenticated Rust
installer. It starts while the Activity is visible and stops on success, failure or cancellation,
even if the Activity is suspended. The installation itself remains Rust. The service does not
restart unattended or bypass account verification. Android 15's six-hour service limit cancels
installation cleanly. Force stop, process termination and vendor battery restrictions can still
stop it; verified files remain available for retry. No battery-optimization exemption is requested.

Manual checks: start downloading, switch apps for a minute, return and confirm progress;
repeat with the display off. The installation notification should disappear when work ends.
Also retry after a connection outage and confirm that verified objects are reused.
