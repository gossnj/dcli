"""Check mobile build-key handling without invoking a compiler or copying libraries."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ("build-apple.sh", "build-android.sh")


class BuildScriptKeyTests(unittest.TestCase):
    def run_script(self, script_name, key):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / script_name
            shutil.copyfile(ROOT / "src/dcli-ffi" / script_name, script)
            tools = root / "tools"
            tools.mkdir()
            marker = root / "invoked"
            # Stop at the first tool boundary; no test library can reach a consumer.
            stub = '#!/bin/sh\n[ "$DESTINY_API_KEY" = "validation-only-key" ] && printf expected > "$BUILD_TEST_MARKER" || printf unexpected > "$BUILD_TEST_MARKER"\nexit 73\n'
            for name in ("cargo", "rustup", "lipo", "cp"):
                path = tools / name
                path.write_text(stub)
                path.chmod(0o755)
            env = os.environ.copy()
            env.pop("DESTINY_API_KEY", None)
            if key is not None:
                env["DESTINY_API_KEY"] = key
            env["PATH"] = os.pathsep.join((str(tools), str(Path.home() / ".cargo/bin"), env["PATH"]))
            env["BUILD_TEST_MARKER"] = str(marker)
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


if __name__ == "__main__":
    unittest.main()
