"""Offline checks for pinned source build, validated cache and failure propagation."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

script = Path(__file__).with_name("install-wasm-opt.sh").resolve()
with tempfile.TemporaryDirectory(prefix="wasm-opt-installer-test-") as temporary:
    root = Path(temporary)
    tools = root / "tools"
    tools.mkdir()
    (tools / "git").write_text('''#!/usr/bin/env python3
import json, os, sys
with open(os.environ['MOCK_LOG'], 'a') as f: f.write(json.dumps(['git'] + sys.argv[1:]) + '\\n')
''')
    (tools / "cmake").write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
with open(os.environ['MOCK_LOG'], 'a') as f: f.write(json.dumps(['cmake'] + sys.argv[1:]) + '\\n')
if sys.argv[1] == '--build':
    if int(os.environ.get('MOCK_BUILD_EXIT', '0')): sys.exit(int(os.environ['MOCK_BUILD_EXIT']))
    path = pathlib.Path(sys.argv[2]) / 'bin' / 'wasm-opt'
    path.parent.mkdir(parents=True)
    path.write_text('#!/usr/bin/env bash\\necho "wasm-opt version ' + os.environ.get('MOCK_VERSION', '130') + '"\\n')
    path.chmod(0o755)
''')
    for tool in tools.iterdir(): tool.chmod(0o755)
    for name, build_exit, version, expected in [
        ("build", 0, "130", 0), ("failed-build", 27, "130", 27),
        ("wrong-version", 0, "129", 1), ("stale-cache", 0, "130", 0)
    ]:
        target, log = root / name, root / f"{name}.log"
        env = dict(os.environ, PATH=f"{tools}:{os.environ['PATH']}", WASM_OPT_ROOT=str(target),
                   MOCK_LOG=str(log), MOCK_BUILD_EXIT=str(build_exit), MOCK_VERSION=version)
        if name == "stale-cache":
            binary = target / "bin/wasm-opt"
            binary.parent.mkdir(parents=True)
            binary.write_text('#!/usr/bin/env bash\necho "wasm-opt version 129"\n')
            binary.chmod(0o755)
        result = subprocess.run(["bash", str(script)], env=env, capture_output=True, text=True)
        assert result.returncode == expected, (name, result.stdout, result.stderr)
        calls = [json.loads(line) for line in log.read_text().splitlines()]
        assert any("5d704ad52bc77a258e8fa3f9d34fcc5e8799c1c3" in call for call in calls)
        assert any("-DBUILD_SHARED_LIBS=OFF" in call for call in calls)
        assert any("wasm-opt" in call and "--target" in call for call in calls)
        if expected == 0:
            result = subprocess.run(["bash", str(script)], env=env, capture_output=True, text=True)
            assert result.returncode == 0
            assert len(log.read_text().splitlines()) == len(calls), "validated cache rebuilt"
print("wasm-opt installer: 4 offline scenarios passed")
