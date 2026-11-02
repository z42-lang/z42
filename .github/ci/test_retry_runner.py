"""Regression coverage for runner loss, fail-closed decisions, and retry races."""

import copy
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

from retry_runner import ACQUISITION_ERROR, retry


REPO = "owner/repo"
PREFIX = f"repos/{REPO}"
RUN_PATH = f"{PREFIX}/actions/runs/42"
JOBS_PATH = f"{RUN_PATH}/jobs?filter=latest&per_page=100"
ANNOTATIONS = f"{PREFIX}/check-runs/7/annotations?per_page=100"


class FakeGitHub:
    def __init__(self):
        self.run = dict(path=".github/workflows/ci.yml", event="pull_request",
                        status="completed", conclusion="failure", run_attempt=1,
                        head_sha="abc", head_branch="fix", head_repository={"full_name": REPO},
                        pull_requests=[{"number": 3}])
        self.pr = dict(state="open", base={"ref": "main", "repo": {"full_name": REPO}},
                       head={"sha": "abc", "ref": "fix", "repo": {"full_name": REPO}})
        self.jobs = [dict(id=7, name="test-stdlib-interp(macos-arm64)", status="completed",
                          conclusion="cancelled", runner_id=0, runner_name="", steps=[],
                          check_run_url=f"https://api.github.com/{PREFIX}/check-runs/7"),
                     dict(id=8, name="ci-ok", status="completed", conclusion="failure"),
                     dict(id=9, name="test-host(linux-x64)", status="completed", conclusion="success")]
        self.annotations = [dict(annotation_level="failure", message=ACQUISITION_ERROR),
                            dict(annotation_level="notice", message="macOS capacity constraints")]
        self.posts = []
        self.gets = 0
        self.on_recheck = lambda: None

    def get(self, path):
        if path == RUN_PATH:
            self.gets += 1
            if self.gets == 2:
                self.on_recheck()
            return copy.deepcopy(self.run)
        if path == f"{PREFIX}/pulls/3":
            return copy.deepcopy(self.pr)
        raise AssertionError(path)

    def pages(self, path, key=None):
        if path == JOBS_PATH and key == "jobs":
            return self.jobs
        if path == ANNOTATIONS and key is None:
            return self.annotations
        raise AssertionError(path)

    def post(self, path):
        self.posts.append(path)


class RetryTests(unittest.TestCase):
    def setUp(self):
        self.api = FakeGitHub()

    def check_skip(self):
        self.assertTrue(retry(self.api, REPO, 42, attempt=1).startswith("Skipped:"))
        self.assertEqual(self.api.posts, [])

    def test_abandoned_matrix_reruns_failed_jobs_and_ci_ok(self):
        result = retry(self.api, REPO, 42, attempt=1)
        self.assertIn("2/3", result)
        self.assertIn("ci-ok", result)
        self.assertEqual(self.api.posts, [f"{RUN_PATH}/rerun-failed-jobs"])

    def test_two_retries_then_stop(self):
        self.api.run["run_attempt"] = 2
        self.assertIn("3/3", retry(self.api, REPO, 42, attempt=2))
        self.api.posts.clear()
        self.api.run["run_attempt"] = 3
        self.assertTrue(retry(self.api, REPO, 42, attempt=3).startswith("Skipped:"))
        self.assertEqual(self.api.posts, [])

    def test_dry_run_does_not_mutate(self):
        self.assertTrue(retry(self.api, REPO, 42, dry_run=True).startswith("Dry run:"))
        self.assertEqual(self.api.posts, [])

    def test_real_test_failure_is_never_retried(self):
        self.api.jobs[0].update(conclusion="failure", runner_id=123, runner_name="hosted",
                                steps=[{"name": "Test", "conclusion": "failure"}])
        self.check_skip()

    def test_partial_execution_is_never_retried(self):
        self.api.jobs[0]["steps"] = [{"name": "Set up job", "conclusion": "success"}]
        self.check_skip()

    def test_mixed_infrastructure_and_test_failures_are_not_retried(self):
        self.api.jobs[-1]["conclusion"] = "failure"
        self.check_skip()

    def test_failure_without_acquisition_annotation_is_not_retried(self):
        self.api.annotations = []
        self.check_skip()

    def test_unrelated_or_additional_failure_annotations_are_not_retried(self):
        for messages in [["Operation was cancelled"], [ACQUISITION_ERROR, "Invalid workflow"]]:
            with self.subTest(messages=messages):
                self.api.annotations = [dict(annotation_level="failure", message=m) for m in messages]
                self.check_skip()

    def test_bad_check_url_is_not_followed(self):
        self.api.jobs[0]["check_run_url"] = "https://example.com/check-runs/7"
        self.check_skip()

    def test_cancelled_or_successful_run_is_not_retried(self):
        for conclusion in ["cancelled", "success", "timed_out"]:
            with self.subTest(conclusion=conclusion):
                self.api.run["conclusion"] = conclusion
                self.check_skip()

    def test_wrong_workflow_or_event_is_not_retried(self):
        self.api.run["event"] = "push"
        self.check_skip()
        self.api.run.update(event="pull_request", path=".github/workflows/other.yml")
        self.check_skip()

    def test_closed_or_updated_pr_is_not_retried(self):
        for changes in [dict(state="closed"), dict(head={"sha": "new"})]:
            with self.subTest(changes=changes):
                self.setUp()
                self.api.pr.update(changes)
                self.check_skip()

    def test_unlinked_pr_or_wrong_target_is_not_retried(self):
        self.api.pr["base"]["ref"] = "release"
        self.check_skip()
        self.setUp()
        self.api.run["pull_requests"] = []
        self.check_skip()

    def test_old_completion_event_is_not_retried(self):
        self.api.run["run_attempt"] = 2
        self.check_skip()

    def test_still_running_job_or_empty_job_list_is_not_retried(self):
        self.api.jobs[0]["status"] = "in_progress"
        self.check_skip()
        self.api.jobs = []
        self.check_skip()

    def test_only_ci_ok_failure_is_not_retried(self):
        self.api.jobs = self.api.jobs[1:]
        self.check_skip()

    def test_retry_or_new_push_during_inspection_is_not_retried(self):
        for attribute, changes in [("run", dict(status="queued", run_attempt=2)),
                                   ("pr", dict(state="closed")),
                                   ("pr", dict(head={"sha": "new"}))]:
            with self.subTest(changes=changes):
                self.setUp()
                target = getattr(self.api, attribute)
                self.api.on_recheck = lambda: target.update(changes)
                self.check_skip()

    def test_all_macos_ci_jobs_share_a_multi_waiter_queue(self):
        workflow = Path(__file__).parents[1] / "workflows" / "ci.yml"
        text = workflow.read_text()
        for name in ["build-and-test", "host-package", "stdlib-interp-consistency", "package-ios", "test-ios"]:
            with self.subTest(job=name):
                section = text.split(f"\n  {name}:\n", 1)[1].split("\n  # ──", 1)[0]
                self.assertIn("concurrency:\n      group:", section)
                self.assertIn("ci-macos-arm64", section)
                self.assertIn("needs.changes.outputs.macos_lane", section)
                self.assertIn("queue: max", section)
                self.assertIn("cancel-in-progress: false", section)

    def test_queue_selection_uses_three_stable_lanes(self):
        workflow = Path(__file__).parents[1] / "workflows" / "ci.yml"
        block = workflow.read_text().split("id: macos_queue\n", 1)[1].split("\n      - ", 1)[0]
        script = textwrap.dedent(block.split("run: |\n", 1)[1])
        lanes = []
        for number in [5566, 5567, 5568, 5566]:
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "output"
                env = dict(os.environ, GITHUB_OUTPUT=str(output))
                subprocess.run(["bash", "-euo", "pipefail", "-c",
                                script.replace("${{ github.run_number }}", str(number))],
                               env=env, check=True, capture_output=True)
                line = output.read_text().strip()
                self.assertTrue(line.startswith("lane="))
                lanes.append(int(line.removeprefix("lane=")))
        self.assertEqual(set(lanes[:3]), {0, 1, 2})
        self.assertEqual(lanes[0], lanes[3])


if __name__ == "__main__":
    unittest.main()
