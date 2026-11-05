"""Execute the composite bootstrap shell against observable SDK command probes."""

import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import textwrap
import unittest


ACTION = Path(__file__).resolve().parents[1] / "actions/ci-bootstrap/action.yml"


def bootstrap_script(rid="linux-x64"):
    lines = ACTION.read_text().splitlines()
    start = lines.index("      run: |") + 1
    body = []
    for line in lines[start:]:
        if line and not line.startswith("        "):
            break
        body.append(line[8:] if line else "")
    script = "\n".join(body).replace("${{ inputs.rid }}", rid)
    if "${{" in script:
        raise AssertionError("bootstrap fixture needs an explicit value for a new action input")
    return script


PROBE = textwrap.dedent("""\
    #!/usr/bin/env python3
    import json
    import os
    from pathlib import Path
    import sys

    root = Path(os.environ["BOOTSTRAP_FIXTURE_ROOT"])
    tool = Path(sys.argv[0]).name
    with (root / "calls.jsonl").open("a") as log:
        log.write(json.dumps({"tool": tool, "args": sys.argv[1:]}) + "\\n")
    if tool == "z42vm":
        # Product-format bridging is outside this test's scope. A sentinel failure
        # proves full mode still enters it and tool-only mode never launches it.
        sys.exit(67)
    if tool == "z42c" and os.environ.get("FAIL_XTASK_COMPILER"):
        sys.exit(23)
    outputs = ["artifacts/xtask/xtask.zpkg"] if tool == "z42c" else [
        "artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg",
        "artifacts/intermediate/libraries/flat/release/z42.core.zpkg",
    ]
    for name in outputs:
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fixture product")
    """)


class BootstrapModeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ci bootstrap ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True,
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for name in [".z42/programs/z42c/z42c.driver.zpkg", ".z42/libs/z42.core.zpkg"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"ZPKG\x00\x00" + struct.pack("<H", 1))
        (self.root / ".z42/seed-id.txt").write_text("fixture-seed\n")
        for name in [".z42/bin/z42c", ".z42/bin/z42vm", ".github/ci/xtask"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(PROBE)
            path.chmod(0o755)
        self.writer = self.root / "src/compiler/z42.package/src/ZpkgWriter.z42"
        self.writer.parent.mkdir(parents=True)
        self.writer.write_text("public static int Minor = 1;\n")
        (self.root / "src/libraries").mkdir(parents=True)

    def run_bootstrap(self, mode=None, fail_compiler=False):
        env = os.environ.copy()
        env.pop("XTASK_ONLY", None)
        env.update(BOOTSTRAP_FIXTURE_ROOT=str(self.root), GITHUB_WORKSPACE=str(self.root))
        if mode is not None:
            env["XTASK_ONLY"] = mode
        if fail_compiler:
            env["FAIL_XTASK_COMPILER"] = "1"
        result = subprocess.run(["bash", "-c", bootstrap_script()], cwd=self.root,
                                env=env, text=True, capture_output=True, timeout=20)
        path = self.root / "calls.jsonl"
        calls = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        return result, calls

    def assert_tool_only(self):
        result, calls = self.run_bootstrap("true")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["z42c"])
        self.assertEqual(calls[0]["args"], ["build", "scripts/xtask.z42.toml", "--release", "--no-incremental"])
        self.assertTrue((self.root / "artifacts/xtask/xtask.zpkg").is_file())
        self.assertEqual((self.root / "artifacts/xtask/seed-id.txt").read_text(), "fixture-seed\n")
        self.assertFalse((self.root / ".ci-seed").exists())
        self.assertFalse((self.root / "artifacts/build/runtime").exists())
        self.assertFalse((self.root / "artifacts/build/libraries").exists())

    def test_tool_only_matching_format(self):
        self.assert_tool_only()

    def test_tool_only_format_gap(self):
        self.writer.write_text("public static int Minor = 2;\n")
        self.assert_tool_only()

    def test_tool_only_does_not_read_product_format(self):
        self.writer.unlink()
        self.assert_tool_only()

    def test_tool_only_still_rejects_missing_xtask_output(self):
        result, calls = self.run_bootstrap("true", fail_compiler=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("xtask.zpkg not produced", result.stdout + result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["z42c"])

    def test_default_mode_builds_products(self):
        result, calls = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["z42c", "xtask", "xtask"])
        self.assertEqual(calls[1]["args"], ["-v", "detailed", "build", "compiler"])
        self.assertEqual(calls[2]["args"], ["-v", "detailed", "build", "stdlib"])
        self.assertTrue((self.root / ".ci-seed/z42c.driver.zpkg").is_file())

    def test_full_mode_keeps_format_bridge(self):
        self.writer.write_text("public static int Minor = 2;\n")
        result, calls = self.run_bootstrap("false")
        self.assertEqual(result.returncode, 67, result.stdout + result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["z42vm"])

    def test_full_mode_rejects_unreadable_product_format(self):
        self.writer.write_text("// no format constant\n")
        result, calls = self.run_bootstrap("false")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
