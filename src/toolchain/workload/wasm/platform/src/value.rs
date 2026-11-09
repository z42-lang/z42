//! `Z42VMValue` ↔ `JsValue` marshaling.
//!
//! v0.1 supports the same shapes as the Tier 2 H2 surface:
//!     null  ←→ JS null / undefined
//!     i64   ←→ JS `bigint` (preferred) or JS `number` if safe-integer
//!     f64   ←→ JS `number`
//!     bool  ←→ JS `boolean`
//!
//! Strings / objects / arrays follow once the runtime gains a string
//! marshal protocol (H3+ — see embedding.md §12 Deferred).

use wasm_bindgen::prelude::*;

use z42_abi::{Z42Value, Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NULL};
use z42::host::rust::Value;

use crate::error::js_error;

/// `Number.MAX_SAFE_INTEGER` — beyond this, plain JS numbers lose
/// precision and we round-trip through BigInt to preserve all 64 bits.
const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Convert a JS value into a Tier 2 [`Value`] for use as a function arg.
pub(crate) fn js_to_value(js: &JsValue) -> Result<Value, JsValue> {
    if js.is_null() || js.is_undefined() {
        return Ok(Value::null());
    }
    if let Some(b) = js.as_bool() {
        return Ok(Value::bool(b));
    }
    if let Some(n) = js.as_f64() {
        // Treat integers that fit in i64 as i64; floats as f64. JS
        // numbers don't distinguish, so the heuristic is "integral
        // value AND within safe-integer range → i64".
        if n.fract() == 0.0 && n.abs() <= JS_MAX_SAFE_INTEGER as f64 {
            return Ok(Value::i64(n as i64));
        }
        return Ok(Value::f64(n));
    }
    // BigInt path — wasm-bindgen exposes it as JsValue.
    if let Ok(big) = js.clone().dyn_into::<js_sys::BigInt>() {
        // BigInt → i64 with overflow check.
        let as_i64 = i64::try_from(big).map_err(|_| {
            js_error(
                "ArgMismatch",
                21,
                "BigInt argument is out of i64 range",
            )
        })?;
        return Ok(Value::i64(as_i64));
    }
    Err(js_error(
        "ArgMismatch",
        21,
        "unsupported Z42VMValue shape (expected null / boolean / number / bigint)",
    ))
}

/// Convert a Tier 2 [`Value`] back into a JS value for return-from-invoke.
pub(crate) fn value_to_js(v: Value) -> JsValue {
    let raw: Z42Value = v.raw();
    match raw.tag {
        Z42_VALUE_TAG_NULL => JsValue::NULL,
        Z42_VALUE_TAG_I64 => {
            let n = raw.payload as i64;
            if n.abs() <= JS_MAX_SAFE_INTEGER {
                JsValue::from_f64(n as f64)
            } else {
                js_sys::BigInt::from(n).into()
            }
        }
        Z42_VALUE_TAG_F64 => JsValue::from_f64(f64::from_bits(raw.payload)),
        Z42_VALUE_TAG_BOOL => JsValue::from_bool(raw.payload != 0),
        _ => JsValue::NULL, // strings / objects / etc. — surface as null in v0.1
    }
}
