"""Check mobile build prerequisites without invoking compilers or copying libraries."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ("build-apple.sh", "build-android.sh")


class BuildScriptKeyTests(unittest.TestCase):
    def run_script(self, script_name, key, tool_overrides=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / script_name
            shutil.copyfile(ROOT / "src/dcli-ffi" / script_name, script)
            tools = root / "tools"
            tools.mkdir()
            cargo_env = root / ".cargo"
            cargo_env.mkdir()
            (cargo_env / "env").write_text("")
            marker = root / "invoked"
            # Stop at the first tool boundary; no test library can reach a consumer.
            stub = '#!/bin/sh\n[ "$DESTINY_API_KEY" = "validation-only-key" ] && printf expected > "$BUILD_TEST_MARKER" || printf unexpected > "$BUILD_TEST_MARKER"\nexit 73\n'
            for name in ("cargo", "rustup", "lipo", "cp"):
                path = tools / name
                path.write_text((tool_overrides or {}).get(name, stub))
                path.chmod(0o755)
            env = os.environ.copy()
            env.pop("DESTINY_API_KEY", None)
            env.pop("BASH_ENV", None)
            if key is not None:
                env["DESTINY_API_KEY"] = key
            env["HOME"] = str(root)
            env["PATH"] = os.pathsep.join((str(tools), os.defpath))
            env["BUILD_TEST_MARKER"] = str(marker)
            env["BUILD_TEST_TARGETS"] = str(root / "installed-targets")
            result = subprocess.run(["/bin/bash", str(script)], cwd=root, env=env,
                                    capture_output=True, text=True, timeout=10)
            return result, marker.read_text() if marker.exists() else None, (root / "build").exists()

    def test_missing_or_empty_key_fails_before_tools_or_artifacts(self):
        for script in SCRIPTS:
            for key in (None, ""):
                with self.subTest(script=script, key=key):
                    result, invoked, created_build = self.run_script(script, key)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIsNone(invoked, "build tools ran without an environment key")
                    self.assertFalse(created_build, "an output directory was created without a key")
                    self.assertIn("DESTINY_API_KEY", result.stderr)

    def test_supplied_key_reaches_the_first_tool_without_being_logged(self):
        for script in SCRIPTS:
            with self.subTest(script=script):
                result, invoked, _ = self.run_script(script, "validation-only-key")
                self.assertEqual(result.returncode, 73)
                self.assertEqual(invoked, "expected", "the environment-provided key was replaced")
                self.assertNotIn("validation-only-key", result.stdout + result.stderr)


    def test_android_installs_all_targets_on_the_pinned_toolchain_before_building(self):
        rustup = '''#!/bin/sh
if [ "$1" = target ] && [ "$2" = add ]; then
    shift 2
    [ "$1" = --toolchain ] && [ "$2" = "$RUSTUP_TOOLCHAIN" ] || exit 74
    shift 2
    printf '%s\n' "$@" >> "$BUILD_TEST_TARGETS"
fi
'''
        cargo = '''#!/bin/sh
for target in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android; do
    grep -qx "$target" "$BUILD_TEST_TARGETS" || exit 74
done
exit 73
'''
        result, _, _ = self.run_script("build-android.sh", "validation-only-key",
                                       {"rustup": rustup, "cargo": cargo})
        self.assertEqual(result.returncode, 73, result.stderr)


if __name__ == "__main__":
    unittest.main()
