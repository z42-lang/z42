//! Host ABI value marshaling: `Z42Value` ↔ runtime `Value`.
//!
//! Spec: docs/internals/src/runtime/embedding.md §4.4 (z42_host_invoke),
//!       interop.md §6 (Z42Value tag dictionary).
//!
//! H2 scope is **null + primitives** (i64 / f64 / bool). Strings, objects,
//! arrays, pinned views, and typerefs are deferred to H3 — the H2
//! hello-world test invokes `Hello.Main` which has signature `() -> void`.

use anyhow::{Result, bail};

use z42_abi::{
    Z42_VALUE_TAG_BOOL, Z42_VALUE_TAG_F64, Z42_VALUE_TAG_I64, Z42_VALUE_TAG_NULL, Z42Value,
};

use crate::object::Value;

/// Convert a host-supplied `Z42Value` into a runtime `Value` for use as
/// an interpreter argument. Unsupported tags surface as an `anyhow`
/// error which the caller maps to `Z42_HOST_ERR_ARG_MISMATCH`.
fn z42_value_to_value(z: &Z42Value) -> Result<Value> {
    match z.tag {
        Z42_VALUE_TAG_NULL => Ok(Value::Null),
        Z42_VALUE_TAG_I64 => Ok(Value::I64(z.payload as i64)),
        Z42_VALUE_TAG_F64 => Ok(Value::F64(f64::from_bits(z.payload))),
        Z42_VALUE_TAG_BOOL => Ok(Value::Bool(z.payload != 0)),
        other => bail!(
            "z42_host_invoke: argument tag {other} is not supported by H2 (null / i64 / f64 / bool only); strings + objects land in H3"
        ),
    }
}

/// Convert a runtime `Value` returned from the interpreter back into a
/// host-visible `Z42Value`. `None` (void return) maps to a NULL-tagged
/// value so callers always get a defined out_result.
pub(super) fn value_to_z42_value(v: Option<&Value>) -> Result<Z42Value> {
    let v = match v {
        Some(v) => v,
        None => return Ok(null_z42_value()),
    };
    match v {
        Value::Null => Ok(null_z42_value()),
        Value::I64(i) => Ok(raw_value(Z42_VALUE_TAG_I64, *i as u64)),
        Value::F64(f) => Ok(raw_value(Z42_VALUE_TAG_F64, f.to_bits())),
        Value::Bool(b) => Ok(raw_value(Z42_VALUE_TAG_BOOL, u64::from(*b))),
        other => bail!(
            "z42_host_invoke: return value of kind {other:?} is not supported by H2 (null / i64 / f64 / bool only); strings + objects land in H3"
        ),
    }
}

/// Canonical NULL-tagged value. Stable across H2 / H3 / H4.
fn null_z42_value() -> Z42Value {
    raw_value(Z42_VALUE_TAG_NULL, 0)
}

pub(super) const fn raw_value(tag: u32, payload: u64) -> Z42Value {
    Z42Value {
        tag,
        reserved: 0,
        payload,
    }
}

/// Apply the primitive-value contract shared by the C and Rust adapters.
pub(super) fn invoke<'a>(
    token: usize,
    entry: usize,
    args: impl Iterator<Item = &'a Z42Value>,
) -> std::result::Result<Z42Value, super::session::Failure> {
    use super::{error::Z42HostStatus, session};
    let mismatch = |error| session::Failure::new(Z42HostStatus::ArgMismatch, format!("{error:#}"));
    let value = session::invoke(token, entry, || {
        args.map(z42_value_to_value)
            .collect::<Result<_>>()
            .map_err(mismatch)
    })?;
    value_to_z42_value(value.as_ref()).map_err(mismatch)
}
