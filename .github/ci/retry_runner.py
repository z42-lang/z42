#!/usr/bin/env python3
"""Retry PR CI only when hosted runner acquisition failed before execution."""

import argparse
import json
import os
from pathlib import Path
import subprocess


MAX_ATTEMPTS = 3
ACQUISITION_ERROR = "The job was not acquired by Runner of type hosted even after multiple attempts"


class GitHub:
    def get(self, path):
        return json.loads(subprocess.check_output(["gh", "api", path], text=True))

    def pages(self, path, key=None):
        pages = json.loads(subprocess.check_output(
            ["gh", "api", "--paginate", "--slurp", path], text=True))
        return [item for page in pages for item in (page[key] if key else page)]

    def post(self, path):
        subprocess.run(["gh", "api", "--method", "POST", path], check=True)


def eligible_run(run, attempt):
    return (run["path"] == ".github/workflows/ci.yml"
            and run["event"] == "pull_request"
            and run["status"] == "completed" and run["conclusion"] == "failure"
            and run["run_attempt"] < MAX_ATTEMPTS
            and (attempt is None or run["run_attempt"] == attempt))


def current_pr(api, prefix, run):
    for link in run["pull_requests"]:
        pr = api.get(f"{prefix}/pulls/{link['number']}")
        if (pr["state"] == "open" and pr["base"]["ref"] == "main"
                and pr["base"]["repo"]["full_name"] == prefix.removeprefix("repos/")
                and pr["head"]["sha"] == run["head_sha"]
                and pr["head"]["ref"] == run["head_branch"]
                and pr["head"]["repo"]
                and pr["head"]["repo"]["full_name"] == run["head_repository"]["full_name"]):
            return True
    return False


def unacquired(api, prefix, job):
    # No steps and no assigned runner: an execution failure must never be retried here.
    if (job["conclusion"] not in {"failure", "cancelled"}
            or job.get("runner_id") != 0 or job.get("runner_name")
            or job.get("steps") != []):
        return False
    check_url = job.get("check_run_url", "")
    check_prefix = f"https://api.github.com/{prefix}/check-runs/"
    if not check_url.startswith(check_prefix):
        return False
    check_id = check_url.removeprefix(check_prefix)
    if not check_id.isdecimal():
        return False
    annotations = api.pages(f"{prefix}/check-runs/{check_id}/annotations?per_page=100")
    errors = [a["message"].strip() for a in annotations if a["annotation_level"] == "failure"]
    return bool(errors) and all(error == ACQUISITION_ERROR for error in errors)


def retry(api, repo, run_id, attempt=None, dry_run=False):
    prefix = f"repos/{repo}"
    run_path = f"{prefix}/actions/runs/{run_id}"
    run = api.get(run_path)
    if not eligible_run(run, attempt) or not current_pr(api, prefix, run):
        return "Skipped: not a failed current PR CI attempt within the retry budget."

    jobs = api.pages(f"{run_path}/jobs?filter=latest&per_page=100", "jobs")
    if not jobs or any(job["status"] != "completed" for job in jobs):
        return "Skipped: jobs are missing or still running."
    bad = [job for job in jobs if job["conclusion"] not in {"success", "skipped"}]
    # ci-ok can fail solely because an upstream matrix was abandoned. All other
    # unsuccessful jobs must independently prove acquisition failure.
    candidates = [job for job in bad if job["name"] != "ci-ok"]
    if not candidates or any(not unacquired(api, prefix, job) for job in candidates):
        return "Skipped: a job failed for a reason other than hosted runner acquisition."
    if any(job["conclusion"] != "failure" for job in bad if job["name"] == "ci-ok"):
        return "Skipped: ci-ok was cancelled rather than reporting an upstream failure."

    # A manual rerun, new commit, or merge may race the inspection above.
    fresh = api.get(run_path)
    if (not eligible_run(fresh, run["run_attempt"])
            or not current_pr(api, prefix, fresh)):
        return "Skipped: the PR or run changed during inspection."
    names = ", ".join(job["name"] for job in candidates)
    message = f"Retry attempt {run['run_attempt'] + 1}/{MAX_ATTEMPTS}: {names}; rerun ci-ok too."
    if dry_run:
        return f"Dry run: {message}"
    # One request requeues all failed/cancelled jobs and their dependents, including
    # ci-ok. Successful jobs and their artifacts remain available to this run.
    api.post(f"{run_path}/rerun-failed-jobs")
    return message


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", type=int)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text()) if not args.run_id else {}
    source = event.get("workflow_run", {})
    run_id = args.run_id or source.get("id") or int(os.environ["RETRY_RUN_ID"])
    message = retry(GitHub(), os.environ["GITHUB_REPOSITORY"], run_id,
                    attempt=source.get("run_attempt"), dry_run=args.dry_run)
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
            summary.write(f"{message}\n")


if __name__ == "__main__":
    main()
