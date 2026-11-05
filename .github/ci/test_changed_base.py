"""Exercise event-specific test bases with real local Git commits and fetches."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("changed-base.sh").resolve()
WORKFLOW = SCRIPT.parents[1] / "workflows/ci.yml"


class ChangedBaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ci test base ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "no-hooks").mkdir()
        self.remote = self.root / "origin.git"
        self.repo = self.root / "checkout"
        self.git("init", "--bare", "-q", str(self.remote), cwd=self.root)
        self.git("init", "-q", str(self.repo), cwd=self.root)
        self.git("remote", "add", "origin", str(self.remote))
        (self.repo / "README.md").write_text("base\n")
        self.commit("base")
        self.base = self.git("rev-parse", "HEAD")
        path = self.repo / "src/libraries/sample/tests/unit.z42"
        path.parent.mkdir(parents=True)
        path.write_text("namespace SampleTests;\n")
        self.commit("one private stdlib test")
        self.head = self.git("rev-parse", "HEAD")
        self.git("push", "-q", "origin", "HEAD:refs/heads/main")

    def git(self, *args, cwd=None):
        return subprocess.check_output(["git", "-c", "user.name=CI Test",
                                        "-c", "user.email=ci@example.invalid",
                                        "-c", "commit.gpgsign=false",
                                        "-c", "core.hooksPath=" + str(self.root / "no-hooks"), *args],
                                       cwd=cwd or self.repo, text=True, stderr=subprocess.PIPE).strip()

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-q", "-m", message)

    def select(self, event, pr_base="", before=""):
        env = os.environ.copy()
        env.update(GITHUB_EVENT_NAME=event, XTASK_PR_BASE=pr_base, XTASK_PUSH_BEFORE=before)
        result = subprocess.run(["bash", str(SCRIPT)], cwd=self.repo, env=env,
                                text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def test_pull_request_uses_pr_base(self):
        self.assertEqual(self.select("pull_request", self.base, self.head), self.base)

    def test_push_uses_before_and_finds_only_the_new_test(self):
        base = self.select("push", self.head, self.base)
        self.assertEqual(base, self.base)
        self.assertEqual(self.git("diff", "--name-only", base, "HEAD"),
                         "src/libraries/sample/tests/unit.z42")

    def test_scheduled_and_manual_runs_remain_full(self):
        self.git("remote", "remove", "origin")
        for event in ["schedule", "workflow_dispatch", "unknown"]:
            with self.subTest(event=event):
                self.assertEqual(self.select(event, self.base, self.base), "")

    def test_missing_or_initial_push_base_remains_full(self):
        self.git("remote", "remove", "origin")
        for before in ["", "0" * 40]:
            with self.subTest(before=before):
                self.assertEqual(self.select("push", before=before), "")

    def test_unfetchable_base_remains_full(self):
        self.assertEqual(self.select("push", before="f" * 40), "")

    def test_fetch_failure_remains_full_even_if_commit_exists_locally(self):
        self.git("remote", "remove", "origin")
        self.assertEqual(self.select("pull_request", pr_base=self.base), "")

    def test_noncommit_object_cannot_select_partial_tests(self):
        blob = self.git("hash-object", "-w", "README.md")
        self.assertEqual(self.select("push", before=blob), "")

    def run_host_step(self, event, exit_code=0):
        # Execute the real literal step, resolving only the GitHub context used
        # by this job. This also exercises the job's declared base environment.
        job = WORKFLOW.read_text().split("\n  build-and-test:", 1)[1].split("\n  host-package:", 1)[0]
        lines = job.splitlines()
        for start, line in enumerate(lines):
            if line.strip() != "run: |":
                continue
            indent = len(line) - len(line.lstrip()) + 2
            body = []
            for line in lines[start + 1:]:
                if line and not line.startswith(" " * indent):
                    break
                body.append(line[indent:] if line else "")
            if "xtask test --no-build" in "\n".join(body):
                break
        else:
            self.fail("host test command block not found")
        context = {
            "matrix.platform": "linux-x64",
            "env.RUN_RUST_RUNTIME": "true" if event in ["schedule", "workflow_dispatch"] else "false",
            "needs.changes.outputs.host_package": "false",
            "github.event_name": event,
            "github.event.pull_request.base.sha": self.base,
            "github.event.before": self.base,
        }

        def resolve(text):
            for key, value in context.items():
                text = text.replace("${{ " + key + " }}", value)
            self.assertNotIn("${{", text)
            return text

        helper = self.repo / ".github/ci/changed-base.sh"
        helper.parent.mkdir(parents=True)
        shutil.copyfile(SCRIPT, helper)
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        command = bin_dir / "xtask"
        command.write_text("#!/usr/bin/env python3\nimport json, os, pathlib, sys\n"
                           "pathlib.Path(os.environ['XTASK_ARGV_FILE']).write_text(json.dumps(sys.argv[1:]))\n"
                           "sys.exit(int(os.environ['XTASK_EXIT']))\n")
        command.chmod(0o755)
        env = os.environ.copy()
        for key in ["XTASK_PR_BASE", "XTASK_PUSH_BEFORE"]:
            env.pop(key, None)
        env.update(GITHUB_EVENT_NAME=event, PATH=str(bin_dir) + os.pathsep + env["PATH"],
                   XTASK_ARGV_FILE=str(self.root / "argv.json"), XTASK_EXIT=str(exit_code))
        header = job.split("    steps:", 1)[0]
        for key, value in re.findall(r"^      (XTASK_[A-Z_]+): (.+)$", header, re.MULTILINE):
            env[key] = resolve(value)
        result = subprocess.run(["bash", "-c", resolve("\n".join(body))], cwd=self.repo,
                                env=env, text=True, capture_output=True, timeout=20)
        return result, json.loads((self.root / "argv.json").read_text())

    def test_workflow_passes_push_before_to_xtask(self):
        result, argv = self.run_host_step("push")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(argv[-2:], ["--changed", self.base])

    def test_workflow_keeps_pr_selection(self):
        result, argv = self.run_host_step("pull_request")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(argv[-2:], ["--changed", self.base])

    def test_workflow_keeps_periodic_full_selection(self):
        result, argv = self.run_host_step("schedule")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("--changed", argv)

    def test_workflow_preserves_test_failure_exit_code(self):
        result, argv = self.run_host_step("push", exit_code=17)
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
