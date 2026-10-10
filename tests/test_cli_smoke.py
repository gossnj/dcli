"""Offline checks for the seven distributed command-line programs."""
import os
from pathlib import Path
import subprocess
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]
BIN_DIR = Path(os.environ.get("DCLI_BIN_DIR", ROOT / "src/target/debug"))
APPS = ("dclia", "dcliad", "dcliah", "dclim", "dclitime", "dclisync", "dclistat")


def run(app, *args):
    suffix = ".exe" if os.name == "nt" else ""
    return subprocess.run([str(BIN_DIR / (app + suffix)), *args],
                          capture_output=True, text=True, timeout=10)


class CliSmokeTests(unittest.TestCase):
    def test_help(self):
        for app in APPS:
            with self.subTest(app=app):
                result = run(app, "--help")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Usage:", result.stdout)

    def test_version(self):
        for app in APPS:
            for flag in ("--version", "-V"):
                with self.subTest(app=app, flag=flag):
                    result = run(app, flag)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertRegex(result.stdout.strip(), rf"^{app} \d+\.\d+\.\d+")

    def test_current_unix_time(self):
        before = int(time.time())
        result = run("dclitime", "--moment", "now", "--time-format", "unix")
        after = int(time.time())
        self.assertEqual(result.returncode, 0, result.stderr)
        timestamp = int(result.stdout.strip())
        self.assertLessEqual(before, timestamp)
        self.assertLessEqual(timestamp, after)


if __name__ == "__main__":
    unittest.main()
