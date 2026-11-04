"""Exercise upload recovery against a stateful fake gh process, without network access."""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

from upload_release_assets import upload_assets


FAKE_GH = """
import hashlib, json, os, sys
from pathlib import Path
state_file = Path(os.environ["UPLOAD_TEST_STATE"])
s = json.loads(state_file.read_text())
if sys.argv[1:4] == ["release", "view", "nightly"]:
    assert sys.argv[4:] == ["--json", "assets"]
    asset = s["calls"][-1]
    views = s.setdefault("views", {})
    views[asset] = views.get(asset, 0) + 1
    state_file.write_text(json.dumps(s))
    mode = s["modes"].get(asset)
    if mode == "receipt-client" or (mode == "receipt-500" and views[asset] == 1):
        print("HTTP 403: denied" if mode == "receipt-client" else "HTTP 500: server error", file=sys.stderr)
        sys.exit(1)
    print(json.dumps({"assets": [{"name": Path(k).name, **v} for k, v in s["assets"].items()]}))
    sys.exit(0)
assert sys.argv[1:4] == ["release", "upload", "nightly"]
assert sys.argv[5:] == ["--clobber"]
asset = sys.argv[4]
s["calls"].append(asset)
attempt = s["calls"].count(asset)
mode = s["modes"].get(asset, "ok")
error = None
if mode == "client":
    error = "HTTP 403: permission denied (EOF while reading response)"
elif mode == "always" or (mode in ["deleted", "eof"] and attempt == 1):
    if mode == "deleted":
        s["assets"].pop(asset, None)
    error = "EOF" if mode == "eof" else "HTTP 500 (https://api.github.com/releases/assets/42)"
if error is None:
    if mode == "always-missing" or (mode == "missing" and attempt == 1):
        s["assets"].pop(asset, None)
    else:
        s["assets"][asset] = {"digest": "sha256:" + hashlib.sha256(Path(asset).read_bytes()).hexdigest(),
                              "size": Path(asset).stat().st_size, "state": "uploaded"}
        if mode == "mismatch" and attempt == 1:
            s["assets"][asset]["digest"] = "sha256:wrong"
state_file.write_text(json.dumps(s))
if error:
    print(error, file=sys.stderr)
    sys.exit(1)
"""


class ReleaseUploadTests(unittest.TestCase):
    def invoke(self, modes):
        with tempfile.TemporaryDirectory() as tmp:
            state = Path(tmp) / "state.json"
            fake = Path(tmp) / "gh.py"
            fake.write_text(textwrap.dedent(FAKE_GH))
            assets = [str(Path(tmp) / n) for n in ["first archive.tar.gz", "second.tar.gz"]]
            for asset in assets:
                Path(asset).write_bytes(b"current content")
            state.write_text(json.dumps(dict(modes={str(Path(tmp) / k): v for k, v in modes.items()},
                calls=[], assets={x: {"digest": "old", "size": 1, "state": "uploaded"} for x in assets})))
            delays = []
            def run(command, **kwargs):
                self.assertEqual(command[0], "gh")
                return subprocess.run([sys.executable, str(fake), *command[1:]],
                                      env={**os.environ, "UPLOAD_TEST_STATE": str(state)}, **kwargs)
            with contextlib.redirect_stderr(io.StringIO()):
                code = upload_assets("nightly", assets, run=run, sleep=delays.append)
            final = json.loads(state.read_text())
            final["calls"] = [Path(k).name for k in final["calls"]]
            final["assets"] = {Path(k).name: ("old" if v["digest"] == "old" else
                "current" if v["digest"] != "sha256:wrong" else "wrong") for k, v in final["assets"].items()}
            return code, final, delays

    def test_delete_committed_before_500_recovers_and_keeps_upload_order(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "deleted"})
        self.assertEqual(code, 0)
        self.assertEqual(state["assets"], {"first archive.tar.gz": "current", "second.tar.gz": "current"})
        self.assertEqual(state["calls"], ["first archive.tar.gz", "first archive.tar.gz", "second.tar.gz"])
        self.assertEqual(delays, [1])

    def test_client_error_stops_without_retry_or_later_upload(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "client"})
        self.assertNotEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz"])
        self.assertEqual(state["assets"]["second.tar.gz"], "old")
        self.assertEqual(delays, [])

    def test_persistent_500_exhausts_budget_and_stays_failed(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "always"})
        self.assertNotEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz"] * 5)
        self.assertEqual(state["assets"]["second.tar.gz"], "old")
        self.assertEqual(delays, [1, 2, 4, 8])

    def test_eof_retries_only_the_incomplete_asset(self):
        code, state, delays = self.invoke({"second.tar.gz": "eof"})
        self.assertEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz", "second.tar.gz", "second.tar.gz"])
        self.assertEqual(delays, [1])

    def test_success_without_asset_receipt_retries(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "missing"})
        self.assertEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz", "first archive.tar.gz", "second.tar.gz"])
        self.assertEqual(delays, [1])

    def test_success_with_wrong_digest_retries(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "mismatch"})
        self.assertEqual(code, 0)
        self.assertEqual(state["assets"], {"first archive.tar.gz": "current", "second.tar.gz": "current"})
        self.assertEqual(delays, [1])

    def test_success_without_receipt_exhausts_budget_and_stays_failed(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "always-missing"})
        self.assertNotEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz"] * 5)
        self.assertEqual(state["assets"]["second.tar.gz"], "old")
        self.assertEqual(delays, [1, 2, 4, 8])

    def test_receipt_transport_failure_retries_the_same_asset(self):
        code, state, delays = self.invoke({"second.tar.gz": "receipt-500"})
        self.assertEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz", "second.tar.gz", "second.tar.gz"])
        self.assertEqual(delays, [1])

    def test_receipt_permission_error_stops_without_retry(self):
        code, state, delays = self.invoke({"first archive.tar.gz": "receipt-client"})
        self.assertNotEqual(code, 0)
        self.assertEqual(state["calls"], ["first archive.tar.gz"])
        self.assertEqual(state["assets"]["second.tar.gz"], "old")
        self.assertEqual(delays, [])


if __name__ == "__main__":
    unittest.main()
