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

/// Print better panic messages in the browser console / Node REPL.
#[wasm_bindgen(start)]
pub fn _init_panic_hook() {
    // No-op in v0.1; users can opt into console_error_panic_hook via
    // their own bundler if they want. We keep this crate dep-light.
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
