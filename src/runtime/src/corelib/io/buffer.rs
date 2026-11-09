//! Shared managed byte buffer access for native I/O builtins.

use crate::exec::call::builtin_abi::BuiltinError;

/// .NET `Stream.Read` / `Write` argument checks against a buffer of `len` bytes.
pub(crate) fn check_buffer_range(offset: i64, count: i64, len: usize) -> Result<(usize, usize), BuiltinError> {
    if offset < 0 {
        return Err(BuiltinError::ArgumentOutOfRange("Non-negative number required. (Parameter 'offset')".into()));
    }
    if count < 0 {
        return Err(BuiltinError::ArgumentOutOfRange("Non-negative number required. (Parameter 'count')".into()));
    }
    let (off, n) = (offset as usize, count as usize);
    if off.checked_add(n).is_none_or(|end| end > len) {
        return Err(BuiltinError::Argument(
            "Offset and length were out of bounds for the array or count is greater than the number of \
             elements from index to the end of the source collection.".into()));
    }
    Ok((off, n))
}

pub(crate) fn byte_array(v: &crate::object::Value, param: &str) -> Result<crate::object::ArrRef, BuiltinError> {
    match v {
        crate::object::Value::Array(rc) => Ok(*rc),
        crate::object::Value::Null => Err(BuiltinError::ArgumentNull(format!("Value cannot be null. (Parameter '{param}')"))),
        other => Err(BuiltinError::InvalidCast(format!(
            "argument `{param}` expects a byte[], got {}", crate::object::value_kind_name(other)))),
    }
}

/// `buf[off..off + n]` copied out (packed `byte[]`: one atomic chunked copy).
pub(crate) fn read_byte_range(arr: &crate::object::ArrayObj, off: usize, n: usize) -> Result<Vec<u8>, BuiltinError> {
    if let Some(b) = arr.byte_range(off, n) { return Ok(b); }
    (off..off + n).map(|i| match arr.get_boxed(i) {
        crate::object::Value::I64(b) if (0..=255).contains(&b) => Ok(b as u8),
        other => Err(BuiltinError::InvalidCast(format!(
            "buffer element {i} is not a byte: {}", crate::object::value_kind_name(&other)))),
    }).collect()
}

/// Store `bytes` into `buf` from `off` on.
pub(crate) fn write_byte_range(arr: &crate::object::ArrayObj, off: usize, bytes: &[u8]) {
    if !arr.write_byte_range(off, bytes) {
        for (i, b) in bytes.iter().enumerate() {
            arr.set_boxed(off + i, crate::object::Value::I64(*b as i64));
        }
    }
}
