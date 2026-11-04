#!/usr/bin/env python3
"""Upload and verify release assets in order, with bounded transient-failure recovery."""

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import time

MAX_ATTEMPTS = 5
HTTP_CLIENT_ERROR = re.compile(r"\bHTTP(?:/\S+)?\s+4\d\d\b", re.I)
TRANSIENT_ERROR = re.compile(
    r"\bHTTP(?:/\S+)?\s+5\d\d\b|\bEOF\b|connection reset|"
    r"TLS handshake timeout|i/o timeout|connection timed out|temporary failure in name resolution", re.I)


def upload_assets(release, assets, *, run=subprocess.run, sleep=time.sleep):
    for asset in assets:
        asset = Path(asset)
        digest = "sha256:" + hashlib.sha256(asset.read_bytes()).hexdigest()
        for attempt in range(MAX_ATTEMPTS):
            # Re-resolve the asset on every attempt: a failed DELETE response can
            # arrive after the old asset was removed. --clobber handles both states.
            result = run(["gh", "release", "upload", release, str(asset), "--clobber"],
                         text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            receipt_missing = False
            if result.returncode == 0:
                result = run(["gh", "release", "view", release, "--json", "assets"],
                             text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                if result.returncode == 0:
                    try:
                        entries = json.loads(result.stdout)["assets"]
                        verified = any(entry.get("name") == asset.name
                                       and entry.get("size") == asset.stat().st_size
                                       and entry.get("digest") == digest
                                       and entry.get("state") == "uploaded" for entry in entries)
                    except (ValueError, KeyError, TypeError, AttributeError):
                        print("Invalid release asset receipt", file=sys.stderr)
                        return 1
                    if verified:
                        break
                    receipt_missing = True
                    result = subprocess.CompletedProcess(result.args, 1,
                        f"Release asset receipt missing or mismatched for {asset.name}\n")
            output = result.stdout or ""
            if output:
                print(output, end="" if output.endswith("\n") else "\n", file=sys.stderr)
            if (attempt + 1 == MAX_ATTEMPTS or HTTP_CLIENT_ERROR.search(output)
                    or not (receipt_missing or TRANSIENT_ERROR.search(output))):
                return result.returncode
            delay = 2 ** attempt
            print(f"Transient upload failure for {asset}; retry {attempt + 2}/{MAX_ATTEMPTS} in {delay}s",
                  file=sys.stderr)
            sleep(delay)
    return 0


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if len(argv) < 2:
        print("usage: upload_release_assets.py RELEASE ASSET [ASSET ...]", file=sys.stderr)
        return 2
    return upload_assets(argv[0], argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
