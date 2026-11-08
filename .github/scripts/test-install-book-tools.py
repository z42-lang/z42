"""Exercise release success, HTTP/HTML failure and failed source fallback offline."""
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

installer = Path(__file__).with_name("install-book-tools.sh").resolve()
with tempfile.TemporaryDirectory(prefix="book-install-test-") as directory:
    root = Path(directory)
    mock_bin = root / "mock-bin"
    mock_bin.mkdir()
    archive = root / "valid.tgz"
    with tarfile.open(archive, "w:gz") as tar:
        data = b"tool fixture"
        info = tarfile.TarInfo("tool-fixture")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    (mock_bin / "curl").write_text('''#!/usr/bin/env bash
set -eu
while [ "$1" != --output ]; do shift; done
output="$2"
case "$MOCK_KIND" in
  http) exit 22 ;;
  html) printf '<html>gateway timeout</html>' > "$output" ;;
  valid) cp "$MOCK_ARCHIVE" "$output" ;;
esac
''')
    (mock_bin / "cargo").write_text('''#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$MOCK_LOG"
exit "${MOCK_CARGO_EXIT:-0}"
''')
    for tool in mock_bin.iterdir():
        tool.chmod(0o755)
    for number, (kind, cargo_exit) in enumerate([
        ("valid", 0), ("http", 0), ("html", 0), ("http", 37)
    ]):
        target = root / str(number)
        log = root / f"{number}.log"
        env = dict(os.environ, PATH=f"{mock_bin}:{os.environ['PATH']}",
                   BOOK_TOOLS_ROOT=str(target), MOCK_KIND=kind,
                   MOCK_ARCHIVE=str(archive), MOCK_LOG=str(log),
                   MOCK_CARGO_EXIT=str(cargo_exit))
        result = subprocess.run(["bash", str(installer)], env=env, capture_output=True, text=True)
        assert result.returncode == cargo_exit, (kind, result.stdout, result.stderr)
        calls = log.read_text().splitlines() if log.exists() else []
        if kind == "valid":
            assert not calls
            assert (target / "bin/tool-fixture").read_bytes() == b"tool fixture"
        else:
            assert len(calls) == (1 if cargo_exit else 2), calls
            assert f"install mdbook --version 0.4.40 --locked --root {target}" == calls[0]
            if not cargo_exit:
                assert f"install mdbook-mermaid --version 0.14.0 --locked --root {target}" == calls[1]
print("book tool installer: 4 offline scenarios passed")
