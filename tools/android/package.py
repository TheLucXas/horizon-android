#!/usr/bin/env python3
"""Package a prebuilt Rust NativeActivity using official Android SDK tools."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser()
p.add_argument('--sdk', type=Path, default=os.environ.get('ANDROID_HOME'))
p.add_argument('--build-tools', type=Path, help='Override tools directory (ARM64 host builds)')
p.add_argument('--library', type=Path, default=ROOT/'target/aarch64-linux-android/release/libhorizon_android.so')
p.add_argument('--java-helper', type=Path, required=True, help='Rust executable installed by Android for resource preparation')
p.add_argument('--cxx-runtime', type=Path, required=True, help='ARM64 libc++_shared.so from the Android NDK')
p.add_argument('--keystore', type=Path, required=True)
p.add_argument('--out', type=Path, default=ROOT/'dist/horizon-android.apk')
a = p.parse_args()
if not a.sdk:
    p.error('--sdk or ANDROID_HOME is required')
sdk = Path(a.sdk)
tools = a.build_tools or sorted((sdk/'build-tools').iterdir())[-1]
platform = sorted((sdk/'platforms').glob('android-*/android.jar'))[-1]
out = a.out.resolve()
out.parent.mkdir(parents=True, exist_ok=True)
unsigned = out.with_name(out.stem+'-unsigned.apk')
aligned = out.with_name(out.stem+'-aligned.apk')
resources = out.with_name(out.stem+'-resources.zip')
subprocess.run([str(tools/'aapt2'), 'compile', '--dir', str(ROOT/'packaging/android/res'), '-o', str(resources)], check=True)
subprocess.run([str(tools/'aapt2'), 'link', '-I', str(platform), '--manifest', str(ROOT/'packaging/android/AndroidManifest.xml'), '-R', str(resources), '-o', str(unsigned)], check=True)
with zipfile.ZipFile(unsigned, 'a', compression=zipfile.ZIP_DEFLATED) as z:
    z.write(a.library, 'lib/arm64-v8a/libhorizon_android.so')
    z.write(a.java_helper, 'lib/arm64-v8a/libhorizon_java.so')
    z.write(a.cxx_runtime, 'lib/arm64-v8a/libc++_shared.so')
    z.write(ROOT/'LICENSE', 'assets/licenses/Horizon-MIT.txt')
    for path in (ROOT/'packaging/android/licenses').glob('*.txt'):
        z.write(path, 'assets/licenses/'+path.name)
    for path in (ROOT/'crates/horizon-theme/fonts').glob('OFL-*.txt'):
        z.write(path, 'assets/licenses/'+path.name)
    z.write(ROOT/'third_party/winit/LICENSE', 'assets/licenses/winit-LICENSE.txt')
subprocess.run([str(tools/'zipalign'), '-f', '-p', '4', str(unsigned), str(aligned)], check=True)
subprocess.run([str(tools/'apksigner'), 'sign', '--ks', str(a.keystore), '--ks-pass', 'pass:android', '--key-pass', 'pass:android', '--out', str(out), str(aligned)], check=True)
subprocess.run([str(tools/'apksigner'), 'verify', '--verbose', str(out)], check=True)
unsigned.unlink()
aligned.unlink()
resources.unlink()
print(out)
