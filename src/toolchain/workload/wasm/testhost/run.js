// z42 wasm embedded test-host harness (add-wasm-testhost G6).
//
// Mirrors the desktop testhost.c flow, but in a browser: mount every asset
// (agent app.zpkg + stdlib zpkgs + test-bundle manifest/zbcs) into the wasm
// in-memory VFS, run the shared z42 test-agent through `runTestApp`
// (→ z42::app::run → Std.Test.Runner), then read the JSON report back out of
// the VFS. The agent + bundle are byte-identical to the desktop run — only the
// host (this file) differs per platform.
//
// Playwright / CI reads `window.__report` (JSON string) once `window.__done`.
import init, { mountAsset, runTestApp, readAsset } from "./z42_wasm.js";

const AGENT = "/app/z42.testagent.zpkg";
const LIBS = "/libs";
const MANIFEST = "/bundle/manifest.json";
const REPORT = "/out/report.json";

// diagnose-mobile-wasm-embed: a wasm panic aborts to a bare "RuntimeError:
// unreachable" JsValue — the real Rust message is emitted separately via the
// panic hook's console.error (lib.rs _init_panic_hook). Collect those so the
// catch below reports the actual cause instead of the opaque trap.
const __panicLogs = [];
{
  const origErr = console.error.bind(console);
  console.error = (...a) => { __panicLogs.push(a.join(" ")); origErr(...a); };
}

function show(text, cls) {
  const el = document.getElementById("out");
  if (el) { el.textContent = text; if (cls) el.className = cls; }
}

async function main() {
  await init();

  // Mount every declared asset into the VFS at its `vfs` path.
  const files = await (await fetch("./files.json")).json();
  for (const f of files) {
    const buf = await (await fetch("./" + f.url)).arrayBuffer();
    mountAsset(f.vfs, new Uint8Array(buf));
  }

  try {
    // args → GetCommandLineArgs(): manifest, format, out-path (report to VFS file).
    runTestApp(AGENT, "", LIBS, [MANIFEST, "json", REPORT]);
    const report = new TextDecoder().decode(readAsset(REPORT));
    show(report, "ok");
    window.__report = report;
  } catch (e) {
    // Prefer the Rust panic message (from the hook's console.error) over the
    // opaque "RuntimeError: unreachable" trap value.
    const detail = __panicLogs.length ? " | " + __panicLogs.join(" ; ") : "";
    show("ERROR: " + e + detail, "fail");
    window.__error = String(e) + detail;
  } finally {
    window.__done = true;
  }
}

main();
