"""Exercise benchmark routing and Criterion baseline selection."""

import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import textwrap
from types import SimpleNamespace as Namespace
import unittest

from test_package_selection import evaluate, job


WORKFLOW = Path(__file__).parents[1] / "workflows/bench-pr.yml"
FIXTURE = "src/benches/testdata/sample.json"
VERDICT = ".github/ci/bench-verdict.sh"
SCENARIO = "src/benches/scenarios/example.z42"


class BenchBaselineTests(unittest.TestCase):
    def test_criterion_compares_only_existing_baselines(self):
        bench_args = [
            shlex.split(args)
            for args in re.findall(r"--bench gc_cycle_bench -- ([^\n]+) \)", WORKFLOW.read_text())
        ]
        # Base saves its samples. Head measures every case and compares shared IDs.
        self.assertEqual(bench_args, [["--save-baseline", "ab-base"], ["--baseline-lenient", "ab-base"]])


class BenchScopeTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.git("init", "-q")
        self.git("config", "user.name", "CI Fixture")
        self.git("config", "user.email", "fixture@example.org")
        self.git("config", "commit.gpgsign", "false")
        for path in (FIXTURE, VERDICT, SCENARIO):
            self.write(path)
        self.base = self.commit()
        self.workflow = WORKFLOW.read_text()
        self.steps = re.split(r"^      - ", job(self.workflow, "bench-regression"), flags=re.M)[1:]

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True, timeout=20).strip()

    def write(self, path, content="before"):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-qm", "fixture")
        return self.git("rev-parse", "HEAD")

    def scope(self, base=None):
        step = next(s for s in self.steps if s.startswith("name: Select benchmark validation\n"))
        script = textwrap.dedent(step.split("        run: |\n", 1)[1].split("\n      #", 1)[0])
        output, summary = self.root / ".ci-output", self.root / ".ci-summary"
        output.write_text("")
        summary.write_text("")
        result = subprocess.run(["bash", "-c", script], cwd=self.root, capture_output=True, text=True, timeout=20,
                                env={**os.environ, "BASE_SHA": self.base if base is None else base,
                                     "GITHUB_OUTPUT": str(output), "GITHUB_STEP_SUMMARY": str(summary)})
        self.assertEqual(result.returncode, 0, result.stderr)
        scope = output.read_text().strip().removeprefix("verdict_only=")
        self.assertIn(scope, ("true", "false"))
        self.assertEqual(bool(summary.read_text()), scope == "true")
        return scope

    def selected(self, scope, gap=""):
        outputs = Namespace(scope=Namespace(outputs=Namespace(verdict_only=scope)),
                            fmtgap=Namespace(outputs=Namespace(gap=gap)))
        selected = []
        for step in self.steps:
            guard = re.search(r"^        if: (.+)$", step, re.M)
            if not guard or evaluate(guard[1].replace("always()", "True"), steps=outputs):
                selected.append(step.splitlines()[0])
        return selected

    def test_fixture_and_doc_change_use_only_the_seed_and_verdict_step(self):
        self.write(FIXTURE, "after")
        self.write("README.md")
        self.commit()
        scope = self.scope()
        self.assertEqual(scope, "true")
        self.assertEqual([name.split(" (", 1)[0] for name in self.selected(scope)], [
            "uses: actions/checkout@v7",
            "name: Select benchmark validation",
            "name: Bootstrap toolchain",
            "name: Verdict logic self-check",
        ])
        self.assertIn("xtask-only: ${{ steps.scope.outputs.verdict_only }}", self.workflow)

    def test_verdict_script_alone_triggers_the_fast_path(self):
        self.write(VERDICT, "after")
        self.commit()
        self.assertEqual(self.scope(), "true")
        self.assertIn(f"- '{VERDICT}'", self.workflow)

    def test_mixed_change_preserves_full_measurement(self):
        for path in (SCENARIO, ".github/workflows/bench-pr.yml", "src/runtime/src/lib.rs",
                     "src/libraries/z42.core/src/Array.z42", "scripts/bench/xtask_bench.z42", "new-input"):
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                self.write(FIXTURE, "after")
                self.write(path, "after")
                self.commit()
                scope = self.scope()
                self.assertEqual(scope, "false")
                self.assertEqual(len(self.selected(scope, "0")), len(self.steps))
                self.assertEqual(len(self.selected(scope, "1")), len(self.steps) - 5)

    def test_rename_across_the_fixture_boundary_keeps_full_measurement(self):
        for src, dest in ((SCENARIO, FIXTURE + ".moved"), (FIXTURE, SCENARIO + ".moved")):
            with self.subTest(src=src):
                self.git("reset", "--hard", self.base)
                self.git("mv", src, dest)
                self.commit()
                self.assertEqual(self.scope(), "false")

    def test_invalid_or_unrelated_base_keeps_full_measurement(self):
        self.write(FIXTURE, "after")
        self.commit()
        unrelated = self.git("commit-tree", "HEAD^{tree}", "-m", "unrelated")
        for base in ("", "0" * 40, unrelated):
            with self.subTest(base=base):
                self.assertEqual(self.scope(base), "false")
