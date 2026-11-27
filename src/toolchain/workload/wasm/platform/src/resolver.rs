//! Bridge the host's synchronous JavaScript file resolver.

use wasm_bindgen::prelude::*;
use z42::host::{FileKind, FileResolver};

pub(crate) struct JsCallbackResolver {
    target: JsValue,
}

// SAFETY: the wasm target calls JavaScript on its owning thread.
unsafe impl Send for JsCallbackResolver {}
unsafe impl Sync for JsCallbackResolver {}

impl JsCallbackResolver {
    pub(crate) fn new(target: JsValue) -> Self {
        Self { target }
    }
}

impl FileResolver for JsCallbackResolver {
    fn resolve(&self, name: &str, kind: FileKind) -> anyhow::Result<Option<Vec<u8>>> {
        let (function, this) = if let Some(function) = self.target.dyn_ref::<js_sys::Function>() {
            (function.clone(), JsValue::NULL)
        } else {
            let method = js_sys::Reflect::get(&self.target, &JsValue::from_str("resolve"))
                .map_err(|error| anyhow::anyhow!("reading resolver method: {error:?}"))?;
            let function = method.dyn_into::<js_sys::Function>()
                .map_err(|_| anyhow::anyhow!("expected a file resolver function"))?;
            (function, self.target.clone())
        };
        let result = function.call2(
            &this,
            &JsValue::from_str(name),
            &JsValue::from(kind as u32),
        ).map_err(|error| anyhow::anyhow!("JavaScript file resolver failed: {error:?}"))?;
        if result.is_null() || result.is_undefined() {
            return Ok(None);
        }
        let bytes = result.dyn_into::<js_sys::Uint8Array>()
            .map_err(|_| anyhow::anyhow!("file resolver must return a Uint8Array or null"))?;
        Ok(Some(bytes.to_vec()))
    }
}
