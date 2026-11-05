"""Exercise the CI producer and both consumers of the golden bytecode mirror."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).parents[1] / "workflows/ci.yml"
PRODUCER = "Assemble .z42 toolchain (package dev-sdk) + bundle goldens"
CONSUMERS = ("Extract bundled goldens (shared compiled asset)", "Extract bundled goldens")


def run_step(name, cwd, *, prefix="", **env):
    section = WORKFLOW.read_text().split(f"\n      - name: {name}\n", 1)[1]
    step = re.split(r"\n(?= {0,6}\S)", section, maxsplit=1)[0]
    script = textwrap.dedent(step.split("        run: |\n", 1)[1])
    return subprocess.run(["bash", "-c", prefix + script], cwd=cwd, env={**os.environ, **env},
                          capture_output=True, text=True, timeout=20)


class GoldenArtifactsTests(unittest.TestCase):
    def test_round_trip_preserves_the_complete_mirror(self):
        language_case = "tests/basic/hello/source.zbc"
        library_case = "libraries/z42.core/release/tests/ordering/source.zbc"
        for cases in [(language_case,), (language_case, library_case)]:
            with self.subTest(cases=cases), tempfile.TemporaryDirectory(prefix="golden transfer ") as tmp:
                root = Path(tmp)
                build = root / "artifacts/build"
                for index, case in enumerate(cases):
                    path = build / case
                    path.parent.mkdir(parents=True)
                    path.write_bytes(bytes([index, 0, 255]))
                unrelated = build / "libraries/z42.core/release/dist/z42.core.zpkg"
                unrelated.parent.mkdir(parents=True)
                unrelated.touch()

                sdk_temp = root / "runner temp"
                result = run_step(PRODUCER, root, RUNNER_TEMP=str(sdk_temp),
                                  prefix='xtask() { mkdir -p "$4"; }\n')
                self.assertEqual(result.returncode, 0, result.stderr)
                for index, consumer in enumerate(CONSUMERS):
                    target = root / str(index)
                    shutil.copytree(sdk_temp / "z42sdk", target / ".z42-dl")
                    result = run_step(consumer, target)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    restored = target / "artifacts/build"
                    files = {str(p.relative_to(restored)): p.read_bytes()
                             for p in restored.rglob("*") if p.is_file()}
                    self.assertEqual(files, {case: bytes([i, 0, 255]) for i, case in enumerate(cases)})

    def test_missing_bundle_fails_both_consumers(self):
        with tempfile.TemporaryDirectory() as tmp:
            for consumer in CONSUMERS:
                with self.subTest(consumer=consumer):
                    self.assertNotEqual(run_step(consumer, tmp).returncode, 0)
