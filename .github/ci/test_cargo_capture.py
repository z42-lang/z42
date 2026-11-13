"""Exercise the actual Cargo stdout transport used beneath cargo-ndk."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[2] / "scripts/src/common/cargo_capture.rs"


class CargoCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.work = tempfile.TemporaryDirectory(prefix="cargo-capture-")
        cls.exe = Path(cls.work.name) / ("capture.exe" if os.name == "nt" else "capture")
        subprocess.run(
            ["rustc", "--edition=2021", "-C", "prefer-dynamic=no", str(SOURCE), "-o", str(cls.exe)],
            check=True, capture_output=True,
        )

    @classmethod
    def tearDownClass(cls):
        cls.work.cleanup()

    def invoke(self, script, record, *args):
        env = dict(os.environ, Z42_CARGO_EXECUTABLE=sys.executable,
                   Z42_CARGO_ARTIFACT_LOG=str(record), PYTHONUTF8="1")
        return subprocess.run([str(self.exe), "-c", script, *args],
                              env=env, capture_output=True)

    def test_plugin_can_consume_stdout_without_losing_artifact_records(self):
        with tempfile.TemporaryDirectory() as work:
            record = Path(work) / "artifacts.jsonl"
            payload = '{"reason":"compiler-artifact","filenames":["unit/lib.rlib"]}\n'
            for flags in [("--message-format=json-render-diagnostics",),
                          ("--message-format", "json-render-diagnostics")]:
                with self.subTest(flags=flags):
                    result = self.invoke(
                        f"import sys; sys.stdout.buffer.write({payload.encode()!r}); sys.stderr.write('diagnostic')",
                        record, *flags,
                    )
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, payload.encode())
                    self.assertEqual(record.read_bytes(), result.stdout)
                    self.assertEqual(result.stderr, b"diagnostic")

    def test_metadata_preflight_and_followup_do_not_replace_build_records(self):
        with tempfile.TemporaryDirectory() as work:
            record = Path(work) / "artifacts.jsonl"
            result = self.invoke("print('metadata')", record, "--format-version=1")
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, ("metadata" + os.linesep).encode())
            self.assertFalse(record.exists())
            record.write_bytes(b"existing build records")
            self.invoke("print('metadata')", record, "--format-version=1")
            self.assertEqual(record.read_bytes(), b"existing build records")

    def test_child_failure_remains_a_failure(self):
        with tempfile.TemporaryDirectory() as work:
            record = Path(work) / "artifacts.jsonl"
            result = self.invoke("import sys; print('build failed'); sys.exit(7)",
                                 record, "--message-format=json")
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout, record.read_bytes())

    def test_output_larger_than_one_buffer_is_preserved(self):
        with tempfile.TemporaryDirectory() as work:
            record = Path(work) / "artifacts.jsonl"
            result = self.invoke("import sys; sys.stdout.write('x' * 100000)",
                                 record, "--message-format=json")
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"x" * 100000)
            self.assertEqual(record.read_bytes(), result.stdout)
