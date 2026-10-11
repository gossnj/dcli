"""Compile and link a C consumer against both distributed header locations."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
LIBRARY = Path(sys.argv.pop(1)).resolve()


class NativeAbiTests(unittest.TestCase):
    def test_public_headers_compile_link_and_handle_null_inputs(self):
        for relative in ("src/dcli-ffi", "src/dcli-ffi/build"):
            with self.subTest(header=relative), tempfile.TemporaryDirectory() as temp:
                output = Path(temp) / "abi-contract"
                compiled = subprocess.run([
                    os.environ.get("CC", "cc"), "-std=c11", "-Wall", "-Wextra", "-Werror",
                    "-I", str(ROOT / relative), str(ROOT / "tests/native_abi_contract.c"),
                    str(LIBRARY), "-o", str(output),
                ], capture_output=True, text=True, timeout=60)
                self.assertEqual(compiled.returncode, 0, compiled.stderr)
                env = os.environ.copy()
                for variable in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
                    env[variable] = str(LIBRARY.parent) + os.pathsep + env.get(variable, "")
                run = subprocess.run([str(output)], env=env, capture_output=True,
                                     text=True, timeout=10)
                self.assertEqual(run.returncode, 0, run.stderr)


if __name__ == "__main__":
    unittest.main()
