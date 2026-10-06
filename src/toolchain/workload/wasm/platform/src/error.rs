//! Translate [`z42_host::HostError`] into a `JsValue` Error object that
//! carries the host status code and message alongside JS-side semantics
//! (`name`, `message`).

use wasm_bindgen::prelude::*;

use z42_host::HostError;

/// Convert a Tier 2 [`HostError`] into a JS-visible `Error` instance.
/// Sets `err.name = "Z42VMError"`, `err.status = <numeric>` and
/// `err.message = <detail>` so callers can branch on the status code.
pub(crate) fn to_js_error(e: HostError) -> JsValue {
    let (name, status, message) = match &e {
        HostError::AlreadyInit(m) => ("AlreadyInit", 1, m.clone()),
        HostError::NotInit(m) => ("NotInit", 2, m.clone()),
        HostError::BadConfig(m) => ("BadConfig", 3, m.clone()),
        HostError::FeatureOff(m) => ("FeatureOff", 4, m.clone()),
        HostError::BadZbc(m) => ("BadZbc", 10, m.clone()),
        HostError::Verification(m) => ("Verification", 11, m.clone()),
        HostError::EntryNotFound(m) => ("EntryNotFound", 20, m.clone()),
        HostError::ArgMismatch(m) => ("ArgMismatch", 21, m.clone()),
        HostError::VmException(m) => ("VmException", 30, m.clone()),
        HostError::Fatal(m) => ("Fatal", 31, m.clone()),
        HostError::Internal(m) => ("Internal", 99, m.clone()),
    };
    js_error(name, status, &message)
}

/// Build a `JsValue::Error` with the given `name` / `status` / `message`.
/// Exposed as a helper so non-HostError failures (bad arg shape, etc.)
/// can construct equivalent error objects.
pub(crate) fn js_error(name: &str, status: u32, message: &str) -> JsValue {
    let err = js_sys::Error::new(message);
    err.set_name(&format!("Z42VM{name}"));
    let obj: JsValue = err.into();
    let _ = js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("status"),
        &JsValue::from_f64(status as f64),
    );
    obj
}
