"""Run the workflow's package conditions against event and role fixtures."""

from pathlib import Path
import re
from types import SimpleNamespace as Namespace
import unittest


WORKFLOW = Path(__file__).parents[1] / "workflows/ci.yml"
PLATFORM_PACKAGES = {"package-ios", "package-android", "package-wasm"}
ALL_PACKAGES = PLATFORM_PACKAGES | {"host-package"}


def job(text, name):
    section = text.split("\n  " + name + ":\n", 1)[1]
    return re.split(r"\n  [a-z][a-z-]*:\n", section, maxsplit=1)[0]


def evaluate(condition, **context):
    expression = condition.removeprefix("${{").removesuffix("}}").strip()
    # These conditions only use context properties, comparisons and && / ||.
    expression = expression.replace("&&", " and ").replace("||", " or ")
    return eval(expression, {"__builtins__": {}}, context)


def selected_jobs(event, sdk="", *, platform=False, sdk_checks=False, examples=False):
    text = WORKFLOW.read_text()
    github = Namespace(event_name=event, ref="refs/heads/main")
    steps = Namespace(
        filter=Namespace(outputs=Namespace(
            platform=str(platform).lower(), sdk_checks=str(sdk_checks).lower(),
            examples=str(examples).lower(),
        )),
        supersede=Namespace(outputs=Namespace(sdk=sdk)),
    )
    outputs = Namespace(sdk=sdk, platform=str(platform).lower())
    # Evaluate the declared package outputs before the downstream job guards.
    changes = job(text, "changes")
    for name, condition in re.findall(r"^      (\w*package): (.+)$", changes, re.M):
        setattr(outputs, name, str(evaluate(condition, steps=steps, github=github)).lower())
    needs = Namespace(changes=Namespace(outputs=outputs))
    return {
        name for name in ALL_PACKAGES | {"publish-nightly"}
        if evaluate(re.search(r"^    if: (.+)$", job(text, name), re.M)[1],
                    github=github, needs=needs)
    }


class PackageSelectionTests(unittest.TestCase):
    def test_push_without_nightly_changes_or_package_roles_skips_packages(self):
        self.assertEqual(selected_jobs("push", sdk="false"), set())

    def test_sdk_and_example_verification_require_desktop_packages(self):
        for event in ["pull_request", "push"]:
            for role in [{"sdk_checks": True}, {"examples": True}]:
                with self.subTest(event=event, role=role):
                    self.assertEqual(selected_jobs(event, sdk="false", **role), {"host-package"})

    def test_platform_verification_still_uses_all_packages(self):
        for event in ["pull_request", "push"]:
            with self.subTest(event=event):
                self.assertEqual(selected_jobs(event, sdk="false", platform=True), ALL_PACKAGES)

    def test_push_with_unpublished_sdk_or_unknown_diff_retains_release_inputs(self):
        for sdk in ["true", ""]:
            with self.subTest(sdk=sdk):
                self.assertEqual(selected_jobs("push", sdk=sdk), ALL_PACKAGES | {"publish-nightly"})

    def test_scheduled_and_manual_coverage(self):
        self.assertEqual(selected_jobs("schedule"), ALL_PACKAGES)
        self.assertEqual(selected_jobs("workflow_dispatch"), ALL_PACKAGES | {"publish-nightly"})

    def test_unrelated_pull_request_skips_packages(self):
        self.assertEqual(selected_jobs("pull_request"), set())
