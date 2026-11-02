"""Run installer entry points in isolated checkouts and standalone locations."""
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="z42 installer ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.repo = self.base / "checkout with spaces"
        (self.repo / "scripts").mkdir(parents=True)
        (self.repo / "src/compiler").mkdir(parents=True)
        (self.repo / "scripts/xtask.z42.toml").touch()
        self.pins = self.repo / "scripts/versions.toml"
        self.pins.write_text('[toolchain.z42]\nlauncher = "1.2.3"\n')
        self.home = self.base / "home"
        self.home.mkdir()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("Z42_")}
        self.env.update(HOME=str(self.home), USERPROFILE=str(self.home), PROCESSOR_ARCHITECTURE="AMD64")

    def adapters(self):
        if os.name != "nt" and shutil.which("sh"):
            yield "sh", ["sh"], ".sh", ["--dry-run"]
        ps = shutil.which("pwsh") or shutil.which("powershell")
        if ps:
            yield "ps", [ps, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"], ".ps1", ["-DryRun"]

    def run_case(self, kind, command, ext, flags, location="repo", pipe=False, extra=(), env=None, cwd=None):
        target = self.repo / "scripts" if location == "repo" else self.base / "download"
        target.mkdir(exist_ok=True)
        script = target / ("install-z42" + ext)
        shutil.copyfile(SCRIPTS / script.name, script)
        args = command + [str(script)] + flags + list(extra)
        data = None
        if pipe:
            data = script.read_text()
            if kind == "sh":
                args = ["sh", "-s", "--"] + flags + list(extra)
            else:
                # Evaluate the unchanged downloaded source without a script file path.
                args = [command[0], "-NoProfile", "-Command",
                        "$body = [Console]::In.ReadToEnd(); Invoke-Expression ('& {' + $body + '} -DryRun')"]
        return subprocess.run(args, input=data, cwd=cwd or self.repo, env=env or self.env,
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def test_modes_overrides_and_invalid_pins(self):
        adapters = list(self.adapters())
        self.assertTrue(adapters, "at least one installer interpreter is required")
        for kind, command, ext, flags in adapters:
            with self.subTest(interpreter=kind):
                hostile = dict(self.env, Z42_HOME=str(self.base / "foreign sdk"), Z42_VERSION="9.9.9")
                p = self.run_case(kind, command, ext, flags, env=hostile)
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("mode: repository", p.stdout)
                self.assertIn("version: 1.2.3", p.stdout)
                self.assertIn(str(self.repo / ".z42"), p.stdout)
                self.assertNotIn("foreign sdk", p.stdout)
                self.assertRegex(p.stdout, r"modify PATH: (0|False)")
                p = self.run_case(kind, command, ext, flags, cwd=self.base)
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("mode: repository", p.stdout)
                self.assertIn(str(self.repo / ".z42"), p.stdout)
                dest = str(self.base / "custom sdk")
                extra = ["--version", "2.3.4", "--dest", dest] if kind == "sh" else ["-Version", "2.3.4", "-Dest", dest]
                p = self.run_case(kind, command, ext, flags, extra=extra)
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("version: 2.3.4", p.stdout)
                self.assertIn(dest, p.stdout)
                # Standalone script invoked FROM a checkout must still select user defaults.
                p = self.run_case(kind, command, ext, flags, location="download")
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("mode: user", p.stdout)
                self.assertIn(str(self.home / ".z42"), p.stdout)
                self.assertRegex(p.stdout, r"modify PATH: (1|True)")
                p = self.run_case(kind, command, ext, flags, location="download", env=hostile)
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("version: 9.9.9", p.stdout)
                self.assertIn("foreign sdk", p.stdout)
                p = self.run_case(kind, command, ext, flags, pipe=True)
                self.assertEqual(p.returncode, 0, p.stdout)
                self.assertIn("mode: user", p.stdout)
                self.pins.write_text('[toolchain.z42]\nlauncher = ""\n')
                p = self.run_case(kind, command, ext, flags)
                self.assertNotEqual(p.returncode, 0, p.stdout)
                self.assertIn("no valid toolchain.z42.launcher", p.stdout)
                self.pins.unlink()
                p = self.run_case(kind, command, ext, flags)
                self.assertNotEqual(p.returncode, 0, p.stdout)
                self.assertIn("versions.toml is missing", p.stdout)
                self.pins.write_text('[toolchain.z42]\nlauncher = "1.2.3"\n')

    @unittest.skipIf(os.name == "nt", "POSIX archive installation; Windows uses real SDK CI smoke")
    def test_offline_install_preserves_workloads_and_profiles(self):
        archive = self.base / "sdk.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            for name, content in [("z42", b"#!/bin/sh\necho 'z42 fixture'\n"), ("bin/z42vm", b"vm")]:
                entry = tarfile.TarInfo(name)
                entry.size = len(content)
                entry.mode = 0o755
                tar.addfile(entry, io.BytesIO(content))
        dest = self.repo / ".z42"
        (dest / "runtimes").mkdir(parents=True)
        (dest / "runtimes/keep").write_text("workload")
        p = self.run_case("sh", ["sh"], ".sh", [], extra=["--archive", str(archive)])
        self.assertEqual(p.returncode, 0, p.stdout)
        self.assertTrue((dest / "z42").is_file())
        self.assertIn('version = "1.2.3"', (dest / "install.toml").read_text())
        self.assertEqual((dest / "runtimes/keep").read_text(), "workload")
        self.assertEqual(list(self.home.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
