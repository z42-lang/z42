//! Bridge a JS-supplied resolver (callable function or `{ resolve(name) }`
//! object) into a Tier 2 [`ZpkgResolver`].

use wasm_bindgen::prelude::*;

use z42::host::rust::ZpkgResolver;

/// Wraps a JS function or object so the runtime can ask it for zpkg
/// bytes. wasm is effectively single-threaded; the `Send + Sync`
/// requirement of `ZpkgResolver` is satisfied via `unsafe impl` and
/// only safe because we never spawn worker threads from inside the
/// runtime in v0.1.
pub(crate) struct JsCallbackResolver {
    /// Either a function `(name: string) => Uint8Array | null` or an
    /// object whose `resolve` property has that signature.
    target: JsValue,
}

// SAFETY: wasm is single-threaded; the runtime never moves resolvers
// across threads in the wasm build. Document for future review.
unsafe impl Send for JsCallbackResolver {}
unsafe impl Sync for JsCallbackResolver {}

impl JsCallbackResolver {
    pub(crate) fn new(target: JsValue) -> Self {
        Self { target }
    }
}

impl ZpkgResolver for JsCallbackResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        let arg = JsValue::from_str(namespace);

        // Path 1: target is a function — call it directly.
        if let Ok(func) = self.target.clone().dyn_into::<js_sys::Function>() {
            let result = func.call1(&JsValue::NULL, &arg).ok()?;
            return uint8array_to_vec(result);
        }

        // Path 2: target is an object with a `resolve` method — duck-type.
        if self.target.is_object() {
            let resolve_fn = js_sys::Reflect::get(&self.target, &JsValue::from_str("resolve")).ok()?;
            let func = resolve_fn.dyn_into::<js_sys::Function>().ok()?;
            let result = func.call1(&self.target, &arg).ok()?;
            return uint8array_to_vec(result);
        }

        None
    }
}

fn uint8array_to_vec(result: JsValue) -> Option<Vec<u8>> {
    if result.is_null() || result.is_undefined() {
        return None;
    }
    let arr = result.dyn_into::<js_sys::Uint8Array>().ok()?;
    Some(arr.to_vec())
}
