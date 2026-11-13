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


def package_context(text, event, sdk="", *, platform=False, sdk_checks=False, examples=False):
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
    return {"github": github, "needs": Namespace(changes=Namespace(outputs=outputs))}


def selected_jobs(event, sdk="", **roles):
    text = WORKFLOW.read_text()
    context = package_context(text, event, sdk, **roles)
    return {
        name for name in ALL_PACKAGES | {"publish-nightly"}
        if evaluate(re.search(r"^    if: (.+)$", job(text, name), re.M)[1], **context)
    }


def host_steps(event, sdk="", *, runner_os="Linux", **roles):
    text = WORKFLOW.read_text()
    host = job(text, "host-package")
    context = package_context(text, event, sdk, **roles)
    if not evaluate(re.search(r"^    if: (.+)$", host, re.M)[1], **context):
        return ""
    selected = []
    for step in re.split(r"^      - ", host, flags=re.M)[1:]:
        condition = re.search(r"^        if: (.+)$", step, re.M)
        if not condition or evaluate(condition[1], runner=Namespace(os=runner_os), **context):
            selected.append(step)
    return "\n".join(selected)


class PackageSelectionTests(unittest.TestCase):
    def test_cargo_artifact_inputs_select_platform_packages(self):
        platform_filter = re.search(
            r"^            platform:\n(.*?)(?=^            \w+:)",
            WORKFLOW.read_text(), re.M | re.S,
        )[1]
        self.assertIn("- 'scripts/src/common/cargo_artifacts.z42'", platform_filter)
        self.assertEqual(selected_jobs("pull_request", platform=True), ALL_PACKAGES)

    def test_push_without_nightly_changes_or_package_roles_skips_packages(self):
        self.assertEqual(selected_jobs("push", sdk="false"), set())

    def test_sdk_and_example_verification_require_desktop_packages(self):
        for event in ["pull_request", "push"]:
            for role in [{"sdk_checks": True}, {"examples": True}]:
                with self.subTest(event=event, role=role):
                    self.assertEqual(selected_jobs(event, sdk="false", **role), {"host-package"})
                    for runner_os in ["Linux", "macOS", "Windows"]:
                        steps = host_steps(event, sdk="false", runner_os=runner_os, **role)
                        self.assertIn("xtask package sdk --no-build --archive --label nightly --verify", steps)
                        self.assertIn("xtask test docs examples --sdk", steps)
                        self.assertIn("Installer smoke (offline, packaged SDK)", steps)
                        self.assertNotIn("xtask package runtime", steps)
                        self.assertNotIn("xtask package workload", steps)
                        self.assertNotIn("C host links the packaged runtime", steps)
                        # A successful main run still provides the SDK seed fallback.
                        self.assertEqual("Upload host archives" in steps, event == "push")

    def test_platform_verification_still_uses_all_packages(self):
        for event in ["pull_request", "push"]:
            with self.subTest(event=event):
                self.assertEqual(selected_jobs(event, sdk="false", platform=True), ALL_PACKAGES)

    def test_full_platform_roles_keep_desktop_runtime_workloads_and_native_smoke(self):
        for event, sdk, roles in [
            ("pull_request", "", {"platform": True, "sdk_checks": True}),
            ("push", "false", {"platform": True}),
            ("push", "true", {}),
            ("push", "", {}),
            ("schedule", "", {}),
            ("workflow_dispatch", "", {}),
        ]:
            for runner_os in ["Linux", "macOS", "Windows"]:
                with self.subTest(event=event, sdk=sdk, roles=roles, runner_os=runner_os):
                    steps = host_steps(event, sdk, runner_os=runner_os, **roles)
                    self.assertIn("xtask package runtime --no-build --archive", steps)
                    self.assertIn("xtask package workload --archive", steps)
                    self.assertIn("xtask package workload test --archive", steps)
                    self.assertEqual("C host links the packaged runtime" in steps, runner_os != "Windows")

    def test_push_with_unpublished_sdk_or_unknown_diff_retains_release_inputs(self):
        for sdk in ["true", ""]:
            with self.subTest(sdk=sdk):
                self.assertEqual(selected_jobs("push", sdk=sdk), ALL_PACKAGES | {"publish-nightly"})

    def test_scheduled_and_manual_coverage(self):
        self.assertEqual(selected_jobs("schedule"), ALL_PACKAGES)
        self.assertEqual(selected_jobs("workflow_dispatch"), ALL_PACKAGES | {"publish-nightly"})

    def test_unrelated_pull_request_skips_packages(self):
        self.assertEqual(selected_jobs("pull_request"), set())
