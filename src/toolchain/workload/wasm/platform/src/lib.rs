//! `@z42/wasm` — WebAssembly facade for the z42 embedding API.
//!
//! Wraps [`z42_host::Host`] so JavaScript / TypeScript hosts can drive
//! a z42 VM in-process inside a browser or Node.js runtime. All actual
//! VM logic lives in `z42_host` + `z42`; this crate is the JS-side
//! glue and the wasm-bindgen surface.
//!
//! Spec: docs/spec/archive/2026-05-12-add-platform-wasm/ (REVISION 2026-05-11),
//!       docs/design/runtime/embedding.md §6 Tier 3 + §11 ZpkgResolver.
//!
//! H4 scope: single VM instance, sync invoke, null / i64 / f64 / bool
//! marshaling. Async, multi-instance, string marshaling tracked in
//! embedding.md §12 Deferred.

use std::sync::Arc;

use wasm_bindgen::prelude::*;

use z42_host::{ExecMode, Host, HostConfig, Value};

mod error;
mod resolver;
mod value;

use crate::error::{js_error, to_js_error};
use crate::resolver::JsCallbackResolver;
use crate::value::{js_to_value, value_to_js};

// Bind `console.error` so the panic hook can surface the Rust panic message +
// location to the browser console (no `console_error_panic_hook` crate — keeps
// this crate dep-light while still emitting a real message before the wasm trap).
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
}

/// Print better panic messages in the browser console / Node REPL.
///
/// diagnose-mobile-wasm-embed: a bare wasm panic aborts to `RuntimeError:
/// unreachable` with NO message — opaque when the embedded test-host (nested
/// `app::run` per golden) traps in-browser. This hook logs the panic's message
/// + `file:line` to `console.error` *before* the trap, so Playwright's console
/// capture (embedded.spec.ts) surfaces the real cause in CI. Dep-light: uses the
/// `console.error` binding above, not the `console_error_panic_hook` crate.
#[wasm_bindgen(start)]
pub fn _init_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!("z42 wasm panic: {info}"));
    }));
}

/// Single-instance z42 VM handle for JavaScript hosts.
#[wasm_bindgen]
pub struct Z42VM {
    host: Host,
}

/// Opaque handle for a loaded `.zbc` module.
#[wasm_bindgen]
pub struct Z42VMModule {
    inner: z42_host::Module,
}

/// Opaque handle for a resolved entry.
#[wasm_bindgen]
pub struct Z42VMEntry {
    inner: z42_host::Entry,
}

#[wasm_bindgen]
impl Z42VM {
    /// Construct a new VM. `options` is a JS object:
    ///
    /// ```ts
    /// {
    ///   zpkgResolver?: (name: string) => Uint8Array | null
    ///                | { resolve(name: string): Uint8Array | null },
    ///   stdoutHandler?: (bytes: Uint8Array) => void,
    ///   stderrHandler?: (bytes: Uint8Array) => void,
    /// }
    /// ```
    ///
    /// `null` or `undefined` options are equivalent to all defaults
    /// (real `console.log` stdout, no resolver — caller must invoke
    /// only on a self-contained .zbc).
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<Z42VM, JsValue> {
        let mut cfg = HostConfig {
            exec_mode: ExecMode::Interp,
            ..Default::default()
        };

        if !options.is_null() && !options.is_undefined() {
            // stdoutHandler
            if let Some(handler) = read_property(&options, "stdoutHandler") {
                cfg.stdout = Some(make_sink(handler));
            }
            // stderrHandler
            if let Some(handler) = read_property(&options, "stderrHandler") {
                cfg.stderr = Some(make_sink(handler));
            }
            // zpkgResolver — accepts both function and { resolve } object
            if let Some(target) = read_property(&options, "zpkgResolver") {
                let resolver: Arc<dyn z42_host::ZpkgResolver> =
                    Arc::new(JsCallbackResolver::new(target));
                cfg.zpkg_resolver = Some(resolver);
            }
        }

        let host = Host::new(cfg).map_err(to_js_error)?;
        Ok(Z42VM { host })
    }

    /// Load a `.zbc` module from bytes. Bytes are copied internally;
    /// caller may reuse the buffer after this call.
    #[wasm_bindgen(js_name = loadZbc)]
    pub fn load_zbc(&self, bytes: &[u8]) -> Result<Z42VMModule, JsValue> {
        let inner = self.host.load_zbc(bytes).map_err(to_js_error)?;
        Ok(Z42VMModule { inner })
    }

    /// Resolve an entry by FQN (e.g. `"App.Main"` or
    /// `"App.Greeter::greet"`).
    #[wasm_bindgen(js_name = resolveEntry)]
    pub fn resolve_entry(
        &self,
        module: &Z42VMModule,
        fqn: &str,
    ) -> Result<Z42VMEntry, JsValue> {
        let inner = self.host.resolve_entry(&module.inner, fqn).map_err(to_js_error)?;
        Ok(Z42VMEntry { inner })
    }

    /// Invoke an entry. `args` is a JS array of `Z42VMValue`s (null /
    /// boolean / number / bigint). Returns the function's return
    /// value (or `null` for void).
    pub fn invoke(
        &self,
        entry: &Z42VMEntry,
        args: JsValue,
    ) -> Result<JsValue, JsValue> {
        let rust_args = parse_args(args)?;
        let result = self.host.invoke(&entry.inner, &rust_args).map_err(to_js_error)?;
        Ok(value_to_js(result))
    }

    /// Mount a zpkg (or any file) into the in-memory VFS at `path`
    /// (add-wasm-vfs-backend). The VFS is the default fs backend on wasm, so once
    /// the stdlib + z42c compiler zpkgs are mounted under (say) `/libs`, setting
    /// `Z42_LIBS=/libs` lets `DepScan` / z42c / `Script.Eval` **compile z42 source
    /// in the browser** — `File.ReadAllBytes` / `Path.Glob` route here, no real fs.
    /// (Runtime module loading is served separately by the `zpkgResolver`.)
    #[wasm_bindgen(js_name = mountFile)]
    pub fn mount_file(&self, path: &str, bytes: &[u8]) {
        z42::corelib::fs_backend::memory::mount(path, bytes.to_vec());
    }

    /// Compile + run a z42 **source string** in the browser (add-wasm-vfs-backend
    /// 阶段 2). `module` is the loaded `z42.scripting` zpkg (`loadZbc` it once).
    /// The source goes in via the VFS (`/input.z42`, bytes — no string marshaling
    /// needed) and the program's output comes out via the configured
    /// `stdoutHandler`/`stderrHandler` (the eval result value / errors are printed
    /// there). Prereq: stdlib + z42c zpkgs mounted under `/libs` (`mountFile`).
    #[wasm_bindgen(js_name = evalIn)]
    pub fn eval_in(&self, module: &Z42VMModule, source: &str) -> Result<(), JsValue> {
        z42::corelib::fs_backend::memory::mount("/input.z42", source.as_bytes().to_vec());
        let entry = self
            .host
            .resolve_entry(&module.inner, "Std.Scripting.evalFromVfs")
            .map_err(to_js_error)?;
        let no_args: [Value; 0] = [];
        self.host.invoke(&entry, &no_args).map_err(to_js_error)?;
        Ok(())
    }

    /// Explicitly tear down the VM. After this, all `Z42VMModule` /
    /// `Z42VMEntry` instances issued by this VM are invalid; subsequent
    /// method calls will throw `Z42VMNotInit`.
    ///
    /// JavaScript GC also drops the underlying `Host` (which runs
    /// `Drop` → `z42_host_shutdown`), but calling `dispose` makes the
    /// lifetime explicit — useful for tests and short-lived hosts.
    pub fn dispose(self) {
        // self consumes Self; Drop fires automatically.
    }
}

/// Read the namespaces a zpkg provides (its `NSPC` section), as a JS array
/// of strings. The stdlib helpers use this to map namespace → bytes from
/// the packages directly — no `index.json`.
#[wasm_bindgen(js_name = readNamespaces)]
pub fn read_namespaces(bytes: &[u8]) -> Result<Vec<String>, JsValue> {
    z42_host::read_zpkg_namespaces(bytes).map_err(to_js_error)
}

// ── Embedded test-host (add-wasm-testhost G6) ─────────────────────────────
//
// A self-contained "run a bundled app entirely from the in-memory VFS" surface,
// distinct from the `Z42VM` handle API. It mirrors the desktop `z42_host_run_app`
// C entry — same `z42::app::run` core — but sources every artifact from the VFS
// (there is no filesystem on wasm) and returns the app's output through a VFS
// file the caller reads back (there is no process stdout on wasm).
//
// Host flow:
//   1. `mountAsset("/app/z42.testagent.zpkg", agentBytes)`
//      `mountAsset("/libs/z42.core.zpkg", …)` + every dep zpkg
//      `mountAsset("/bundle/manifest.json", …)` + every test `.zbc`
//   2. `runTestApp("/app/z42.testagent.zpkg", "", "/libs",
//                  ["/bundle/manifest.json", "json", "/out/report.json"])`
//   3. `const json = new TextDecoder().decode(readAsset("/out/report.json"))`
//
// Because loader / lazy-loader / app-run reads route through the fs backend
// (loader.rs, lazy_loader.rs, app.rs — all VFS on wasm), the mounted zpkgs and
// test zbcs load exactly as files would on desktop; the runner is the same z42
// bytecode agent.

/// Mount a file (zpkg / zbc / manifest) into the in-memory VFS at `path`. The
/// VFS is process-global, so this needs no VM instance — call it before
/// [`run_test_app`]. Bytes are copied; the caller may reuse the buffer.
#[wasm_bindgen(js_name = mountAsset)]
pub fn mount_asset(path: &str, bytes: &[u8]) {
    z42::corelib::fs_backend::memory::mount(path, bytes.to_vec());
}

/// Read a file back out of the in-memory VFS (e.g. the agent's report file).
/// Returns the raw bytes; decode as UTF-8 on the JS side.
#[wasm_bindgen(js_name = readAsset)]
pub fn read_asset(path: &str) -> Result<Vec<u8>, JsValue> {
    z42::corelib::fs_backend::active()
        .read(path)
        .map_err(|e| js_error("IoError", 30, &format!("{e:#}")))
}

/// Run a self-contained app (`.zbc` / `.zpkg`) already mounted in the VFS,
/// through the shared [`z42::app::run`] core (interp on wasm — no JIT).
///
/// - `app_path`  VFS path of the app (e.g. the test-agent app.zpkg)
/// - `entry`     entry FQN override, or `""` for the app's baked-in entry
/// - `libs_dir`  VFS dir holding `z42.core.zpkg` + deps, or `""` for none
/// - `args`      program args → `GetCommandLineArgs()` (target, format, out-path)
#[wasm_bindgen(js_name = runTestApp)]
pub fn run_test_app(
    app_path: &str,
    entry: &str,
    libs_dir: &str,
    args: Vec<String>,
) -> Result<(), JsValue> {
    let entry_opt = if entry.is_empty() { None } else { Some(entry) };
    let libs = if libs_dir.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(libs_dir))
    };
    let opts = z42::app::RunOpts {
        mode: z42::app::default_mode(),
        libs_dir: libs,
        program_args: args,
        print_stats: false,
        stats_json: false,
    };
    z42::app::run(app_path, entry_opt, opts)
        .map_err(|e| js_error("VmException", 40, &format!("{e:#}")))
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn read_property(obj: &JsValue, key: &str) -> Option<JsValue> {
    let v = js_sys::Reflect::get(obj, &JsValue::from_str(key)).ok()?;
    if v.is_null() || v.is_undefined() {
        None
    } else {
        Some(v)
    }
}

/// Wrap a JS function (or any callable) as a sink closure.
fn make_sink(js_fn: JsValue) -> Box<dyn Fn(&[u8]) + Send + Sync + 'static> {
    // Best-effort cast to Function; if the caller passed something
    // un-callable we silently no-op rather than crash the VM. They'll
    // notice during testing because their handler never fires.
    let func = js_fn.dyn_into::<js_sys::Function>().ok();
    Box::new(move |bytes: &[u8]| {
        if let Some(f) = &func {
            let arr = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
            arr.copy_from(bytes);
            let _ = f.call1(&JsValue::NULL, &arr);
        }
    })
}

fn parse_args(args: JsValue) -> Result<Vec<Value>, JsValue> {
    if args.is_null() || args.is_undefined() {
        return Ok(Vec::new());
    }
    let arr = args.dyn_into::<js_sys::Array>().map_err(|_| {
        js_error("ArgMismatch", 21, "invoke args must be an Array")
    })?;
    let len = arr.length() as usize;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let js_val = arr.get(i as u32);
        out.push(js_to_value(&js_val)?);
    }
    Ok(out)
}
