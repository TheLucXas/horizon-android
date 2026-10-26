#!/usr/bin/env python3
"""Isolated preparation-launcher smoke test; never launches Minecraft or opens saves."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("helper", type=Path)
    parser.add_argument("runtime", type=Path)
    args = parser.parse_args()
    helper, runtime = args.helper.resolve(), args.runtime.resolve()
    env = dict(os.environ)
    env["JAVA_HOME"] = str(runtime)
    env["LD_LIBRARY_PATH"] = os.pathsep.join(map(str, [
        runtime / "lib/server", runtime / "lib", helper.parent,
    ]))
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"]:
        env.pop(key, None)

    def run(mode, *parameters, success=True):
        result = subprocess.run(
            [str(helper), str(runtime), mode, *map(str, parameters)],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, timeout=60,
        )
        if (result.returncode == 0) != success:
            raise RuntimeError(f"{mode}: unexpected exit {result.returncode}\n{result.stdout}")
        return result.stdout

    if "javac 25" not in run("javac", "-version"):
        raise RuntimeError("The helper did not report javac 25")
    with tempfile.TemporaryDirectory(prefix="horizon-java-helper-") as folder:
        folder = Path(folder)
        source = folder / "PreparationProbe.java"
        source.write_text('''public final class PreparationProbe {
            public static void main(String[] args) {
                if (!"true".equals(System.getProperty("java.awt.headless")))
                    throw new AssertionError("Headless preparation required");
                if (Runtime.version().feature() != 25)
                    throw new AssertionError("Wrong Java version");
                if (args.length != 1 || !args[0].equals("argument with spaces"))
                    throw new AssertionError("Argument changed");
                System.out.println("HORIZON_PREPARATION_JVM_OK");
            }
        }''', encoding="utf-8")
        run("javac", "--release", "25", "-proc:none", "-d", folder, source)
        result = run("java", "-cp", folder, "PreparationProbe", "argument with spaces")
        if "HORIZON_PREPARATION_JVM_OK" not in result:
            raise RuntimeError("Compiled class did not finish")
        source.write_text("this is invalid Java", encoding="utf-8")
        run("javac", "--release", "25", source, success=False)
        run("java", "-cp", folder, "NonexistentClass", success=False)
    print("PASS: javac probe, Java25 compile/run, argument preservation, headless VM, failure exits")


if __name__ == "__main__":
    main()
