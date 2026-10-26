# Android resource preparation

Horizon Android's intended first-run sequence is Microsoft sign-in, a successful
Minecraft Services ownership/profile check, official game-file downloads, local
resource preparation, and only then gameplay. Authentication and download errors
must keep that gate closed. The Android port's Microsoft application registration
currently requires Mojang approval; a successful browser sign-in alone does not
satisfy the gate.

The source repository and APK must not contain client/server jars, asset objects,
prepared resource tables, copied textures, private sessions or signing keys.
`research-lock.json` contains hashes and metadata, not game binaries. Preserve the
upstream MIT license and the licenses/notices of each third-party dependency; the
project should not be described as "copyright-free".

## Why the client jar is not enough

`horizon-prepare` builds the complete schema-7 resource folder. Besides extracting
textures, sounds and JSON, it compiles Horizon's original Java exporters and runs
them against the player's verified Mojang jars. These exporters generate collision,
movement, registries, entity models, creative tabs and other tables. A downloader
that merely saves `client.jar` is not a functional preparer. The Android path must
run the same exporters and validate the resulting resource manifest before opening
the Rust game.

The actual game/simulation stays Rust. OpenJDK is used in separate, short-lived
setup processes, not as Minecraft's game runtime. Each exporter is reaped on normal
exit, cancellation or its one-hour timeout. Existing exporters request up to a
2 GiB Java heap; a device must have enough free memory and storage during setup.

## Android execution boundary

Android apps targeting API 29+ cannot execute downloaded binaries from writable
app directories. Reducing target SDK or requiring root is not a solution.

`tools/android/java-helper` is an original Rust executable installed by the package
manager as `lib/arm64-v8a/libhorizon_java.so` in the APK's native library directory.
It loads a separately downloaded, verified Android OpenJDK `lib/libjli.so` and calls
OpenJDK's `JLI_Launch` interface. It never spawns the downloaded `bin/java` or
`bin/javac`. Compiler mode launches `jdk.compiler/com.sun.tools.javac.Main`.

`horizon_prepare::java::Jdk::android(25, runtime_home, helper_path)` probes the
compiler through this bridge. Pass its result to
`horizon_prepare::prepare_with_jdk(options, jdk, log)`. The optional
`jdk.with_cancel(Arc<AtomicBool>)` interrupts and reaps a running exporter when setup
is cancelled, the session is revoked, or the setup window closes.

Environment changes are child-local: `JAVA_HOME` names the extracted runtime;
`LD_LIBRARY_PATH` starts with `runtime/lib/server:runtime/lib` and then the APK's
native directory. **The leading order matters:** stock OpenJDK otherwise tries to
re-execute its launcher to adjust the path. Java option-injection environment
variables are removed. No `set_var` calls mutate the game's process environment.

OpenJDK's `JLI_Launch` is an implementation ABI rather than the stable JNI API.
The runtime archive must remain pinned and any runtime update must repeat the
helper and complete-preparation checks.

## OpenJDK runtime and licenses

The inspected ARM64 runtime is:

- Release: <https://github.com/AngelAuraMC/angelauramc-openjdk-build/releases/tag/download_jre25>
- Archive: `jre25-android-arm64.tar.xz`, 38,031,580 bytes.
- SHA-256: `d3eb7afe2240c26728a1bb440502c5f18ac3883e932d202dd7f0c9bcbbce4c37`.
- `release`: Java 25.0.5, `OS_ARCH=aarch64`; modules include `jdk.compiler`.
- Runtime sources/build information: the release's linked build workflow and
  <https://github.com/FCL-Team/Android-OpenJDK-Build/tree/7a0266e745d9b4acf400afa189b58e672900f710>.

This is an **open-source Android runtime supplied by a third party**, not a Mojang
or Microsoft game-file endpoint. Its GPLv2/ClassPath-exception license and other
notices are in the archive's `legal/` tree and must be retained and accessible.
Archive extraction must reject traversal and escaping links; legitimate in-tree
license symlinks must resolve safely or have their target notice bytes copied.
A release must provide accurate corresponding-source information and must not
claim that all setup files come from Mojang.

The archive does not include `libc++_shared.so`, although `libjli.so` and `libjvm.so`
require it. Package the compatible ARM64 Android NDK C++ runtime beside the helper
and retain the NDK's LLVM/libc++ license and notice files. Do not borrow a binary
or a session from another installed launcher.

## Verification status

Implemented and checked independently of the game:

- The helper builds for `aarch64-linux-android` (debug compilation).
- The helper launches Linux OpenJDK 25 using the same `JLI_Launch` ABI.
- `tools/android/test_java_helper.py HELPER RUNTIME_HOME` compiles and runs an
  original Java 25 probe, checks headless mode and argument preservation, and
  verifies nonzero exit status for compile errors and missing classes.
- The preparer's pre-existing 30 library tests pass after the explicit-JDK split.
- Cancellation has a passing subprocess regression test.
- The original `VanillaEntityModelExporter` compiled and ran through the helper on
  Linux OpenJDK25 with only portable Java dependencies; it produced a 2,465,789-byte
  model table in an isolated fixture. This is one exporter, not the full pipeline.
- `tools/android/java-fixture` builds as a separate NativeActivity test application
  (`dev.horizon.javaprobe`, API35, no internet permission). Its debug APK contains
  only original fixture/helper code and the open-source C++ runtime. It copies an
  explicitly supplied runtime into its own private data and tests javac/run/error
  behavior without accounts or player data. The first stage runs only an original
  probe class; an optional explicitly populated `preparation/` directory runs the
  original exporter pipeline on an isolated developer fixture.
- The target35 fixture passed on the Android device with the package-manager
  installed helper and runtime copied into the fixture's private data: real javac25
  compiled the original probe, java25 ran it, and missing-class failure propagated.
  Root-copied fixture inputs initially needed readable file permissions; this was
  fixture provisioning, not an executable-storage workaround.

The early helper-only checks did not establish complete preparation. The complete
exporter pipeline has since passed in the isolated sandbox described below. This
still does not establish the complete authenticated first-run application flow. In particular, vanilla class initializers may request
platform-native dependencies (for example LWJGL or Tracy). Desktop Linux native
classifiers are not Android libraries. Fail visibly on any such dependency and
resolve it with compatible, licensed components; do not fabricate tables, silently
skip exporters, or ship pre-generated Mojang resources as a fallback.

The developer fixture also provides `src/bin/prepare_probe.rs`, packaged only in
its test APK as `libhorizon_prepare_probe.so`. It takes explicit absolute fixture,
runtime and package-manager installed helper paths, isolates its working directory
and data environment, and invokes the same `prepare_with_jdk` implementation. This
allows a foreground device-command session to exercise the complete Android
exporters without relying on a background Activity that Android may freeze. It
never starts gameplay or uses an account. A joined 105-second watchdog requests
cancellation through `Jdk::with_cancel`; the active exporter is killed and reaped,
and subsequent exporters cannot launch. A deadline returns failure rather than a
success marker. This leaves room before the device-command tool's 120-second
limit. Use an optimized release build for full-preparation timing: debug builds
spend substantial time hashing and indexing the game jars before any exporter.
Rust-only filesystem work checks the deadline when preparation returns;
the watchdog is not a general-purpose preemptive filesystem timeout.
A device-command run outside the app's
sandbox must be reported as such; it complements the real sandbox JVM test rather
than replacing a complete first-run test in the production application.

The final test must demonstrate valid output hashes/schema, interruption recovery,
space/hash failures, and no gameplay or game-resource preparation before verified
ownership. An approved Microsoft application ID is also needed for the real login
end-to-end check.

## Completed Android sandbox preparation check

The complete original preparation pipeline passed on the Android device in the
normal UID of an isolated target35 app, using a temporary dataSync foreground
service so Android could keep it running while the phone was in use. The fixture
read only its own copied official inputs, original exporters and private OpenJDK.
It produced the schema manifest, all exporter tables and 4,962 sound files
(377.3 MB). The service reported COMPLETED and
HORIZON_FULL_ANDROID_PREPARATION_OK, then stopped itself. No player worlds or
account credentials were used.

This establishes the Android preparation/JVM path, not real Microsoft approval or
premium-server login. The production installer's downloader has separate mock
regression tests; its entire authenticated first-run flow still needs a real
account after Mojang approves the Client ID. Background preparation in the
production NativeActivity may pause when Android freezes it; retries retain
verified inputs. The temporary foreground service belongs only to the development
fixture, and is not included in the game APK.
