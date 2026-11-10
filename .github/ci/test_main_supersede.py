"""Exercise the SDK diff decision with GitHub compare response fixtures."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


SCRIPT = Path(__file__).with_name("main-supersede.sh").resolve()
WORKFLOW = SCRIPT.parents[1] / "workflows/ci.yml"


class MainSupersedeTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="ci sdk compare ")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.response = self.root / "compare.json"
        self.pr_response = self.root / "pr.json"
        gh = self.root / "gh"
        gh.write_text(
            "#!/bin/sh\n"
            'case "$*" in\n'
            '  "api repos/fixture/repo/compare/nightly...head") cat "$TEST_COMPARE_RESPONSE" ;;\n'
            '  "api --paginate repos/fixture/repo/pulls/17/files -q "*) jq -r "$5" "$TEST_PR_RESPONSE" ;;\n'
            '  *) exit 64 ;;\n'
            'esac\n'
        )
        gh.chmod(0o755)

    def run_compare(self, args, files, **context):
        self.response.write_text(json.dumps({"files": files}))
        self.pr_response.write_text(json.dumps(files))
        env = os.environ.copy()
        env.update(
            PATH=str(self.root) + os.pathsep + env["PATH"],
            GITHUB_REPOSITORY="fixture/repo",
            TEST_COMPARE_RESPONSE=str(self.response),
            TEST_PR_RESPONSE=str(self.pr_response),
        )
        env.update(context)
        result = subprocess.run(
            ["bash", *args],
            cwd=self.root, env=env, text=True, capture_output=True, timeout=10,
            check=True,
        )
        return result.stdout.strip()

    def sdk(self, *files):
        return self.run_compare([str(SCRIPT), "sdk", "nightly", "head"], files)

    def docs_only(self, event, *files):
        # Run the workflow's literal Bash step with local API response fixtures.
        step = WORKFLOW.read_text().split("      - name: Docs-only change?\n", 1)[1]
        lines = step.split("        run: |\n", 1)[1].splitlines()
        body = []
        for line in lines:
            if line and not line.startswith(" " * 10):
                break
            body.append(line)
        script = textwrap.dedent("\n".join(body)).replace(
            "${{ github.event.before }}", "nightly"
        ).replace("${{ github.event.pull_request.number }}", "17")
        self.assertNotIn("${{", script)
        output = self.root / "output"
        output.write_text("")
        printed = self.run_compare(["-c", script], files, GITHUB_EVENT_NAME=event,
                                   GITHUB_SHA="head", GITHUB_OUTPUT=str(output))
        self.assertEqual(printed, output.read_text().strip())
        return printed

    def test_rename_across_sdk_boundary_in_either_direction(self):
        source = "scripts/src/package/verify.z42"
        test = "scripts/src/test/sdk/verify.z42"
        for old, new in [(source, test), (test, source)]:
            with self.subTest(old=old, new=new):
                self.assertEqual(
                    self.sdk({"status": "renamed", "filename": new, "previous_filename": old}),
                    "true",
                )

    def test_xtask_layout_publish_classification(self):
        for path, expected in [
            ("scripts/src/common/repository.z42", "true"),
            ("scripts/src/build/seed.z42", "true"),
            ("scripts/src/build/stdlib.z42", "true"),
            ("scripts/src/package/desktop.z42", "true"),
            ("scripts/src/setup/install.z42", "true"),
            ("scripts/src/main.z42", "true"),
            ("scripts/install-z42.sh", "true"),
            ("scripts/xtask.z42.toml", "true"),
            ("scripts/src/test/gate.z42", "false"),
            ("scripts/src/test/check/layout.z42", "false"),
            ("scripts/src/test/sdk/verify.z42", "false"),
            ("scripts/src/bench/bench.z42", "false"),
            ("scripts/src/common/harness/fixtures.z42", "false"),
        ]:
            with self.subTest(path=path):
                self.assertEqual(self.sdk({"status": "modified", "filename": path}), expected)

    def test_rename_within_private_tests_does_not_publish(self):
        self.assertEqual(self.sdk({
            "status": "renamed",
            "previous_filename": "src/libraries/z42.io/tests/old.z42",
            "filename": "src/libraries/z42.io/tests/new.z42",
        }), "false")

    def test_files_without_previous_paths(self):
        self.assertEqual(self.sdk({"status": "removed", "filename": "src/runtime/src/lib.rs"}), "true")
        self.assertEqual(self.sdk({"status": "modified", "filename": "docs/roadmap.md"}), "false")
        self.assertEqual(self.sdk(), "false")

    def test_docs_only_uses_both_rename_paths_on_pr_and_push(self):
        new = "docs/internals/src/devinfra/new.md"
        for event in ["pull_request", "push"]:
            for old, expected in [
                ("scripts/src/build/old.z42", "false"),
                ("docs/learn/src/old.md", "false"),
                ("docs/internals/src/devinfra/old.md", "true"),
            ]:
                with self.subTest(event=event, old=old):
                    self.assertEqual(
                        self.docs_only(event, {"status": "renamed", "filename": new, "previous_filename": old}),
                        "docs_only=" + expected,
                    )
