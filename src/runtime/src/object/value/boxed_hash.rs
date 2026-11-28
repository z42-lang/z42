//! Content hashing for a boxed value-struct payload.

use super::Value;

const FNV_OFFSET: u32 = 2_166_136_261;
const FNV_PRIME: u32 = 16_777_619;

fn is_negative_zero(bytes: &[u8]) -> bool {
    bytes.split_last().is_some_and(|(&sign, rest)| {
        sign == 0x80 && rest.iter().all(|&byte| byte == 0)
    })
}

/// Canonicalize negative zero in the float lanes of a value snapshot. Struct equality
/// treats both zeros as equal, so the default hash must see the same bytes for both.
pub fn normalize_signed_zero(
    bytes: &mut [u8], float_fields: &[std::ops::Range<usize>],
) -> anyhow::Result<()> {
    for range in float_fields {
        let field = bytes.get_mut(range.clone())
            .ok_or_else(|| anyhow::anyhow!("struct hash: invalid float field range {range:?}"))?;
        if is_negative_zero(field) {
            field[field.len() - 1] = 0;
        }
    }
    Ok(())
}

/// Hash borrowed arena storage, substituting zero for the sign byte of each negative zero.
/// Float ranges are cached in order of increasing offset and do not overlap.
#[inline]
pub fn hash_struct_with_floats(
    bytes: &[u8], refs: &[Value], float_fields: &[std::ops::Range<usize>],
) -> anyhow::Result<i64> {
    let mut h = FNV_OFFSET;
    let mut start = 0;
    for range in float_fields {
        let field = bytes.get(range.clone())
            .ok_or_else(|| anyhow::anyhow!("struct hash: invalid float field range {range:?}"))?;
        if is_negative_zero(field) {
            let sign = range.end - 1;
            h = hash_bytes(h, &bytes[start..sign]).wrapping_mul(FNV_PRIME);
            start = range.end;
        }
    }
    Ok(hash_refs(hash_bytes(h, &bytes[start..]), refs))
}

/// add-struct-value-eq-fastpath：payload 级哈希——boxed、frame arena `StructRef`、
/// `struct[]` 元素三种表示共用同一实现，同值 struct 无论从哪种表示进来哈希都相同
/// （值相等 ⇒ 哈希相等这条契约跨表示成立）。
pub fn hash_struct_payload(bytes: &[u8], refs: &[Value]) -> i64 {
    hash_refs(hash_bytes(FNV_OFFSET, bytes), refs)
}

fn hash_bytes(mut h: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        h = (h ^ u32::from(byte)).wrapping_mul(FNV_PRIME);
    }
    h
}

fn hash_refs(mut h: u32, refs: &[Value]) -> i64 {
    for r in refs {
        let value = r.unproject_interface();
        // Erased type parameters store scalar values in reference slots. Integer
        // boxes compare equal to their unboxed representation.
        let scalar = match value {
            Value::BoxedStruct(boxed) => boxed.borrow().boxed_scalar().unwrap_or(value),
            _ => value,
        };
        let rh: u32 = match scalar {
            Value::I64(i) => hash_bytes(FNV_OFFSET, &i.to_le_bytes()),
            Value::F64(f) => hash_bytes(FNV_OFFSET, &(if f == 0.0 { 0 } else { f.to_bits() }).to_le_bytes()),
            Value::Bool(b) => u32::from(b),
            Value::Char(c) => c as u32,
            Value::Str(s) => hash_bytes(FNV_OFFSET, s.as_bytes()),
            Value::Object(gc) if gc.type_desc().is_zero_payload_struct() => 0,
            // add-value-hash-ref-leaves：object/array 叶子此前贡献常量 0——Equals 按引用身份比较，
            // 哈希却全同 ⇒ 引用叶子互异的 struct 全部落进同一个桶（O(n) 探测链）。改贡献地址身份
            // 哈希（`identity_hash`，fmix64 混合；GC 不移动对象故稳定）：身份相等 ⇒ 哈希相等（契约
            // 成立），不同身份 → 良好散布。null/委托叶子仍贡献常量：null 相等语义下合法；委托
            // （FuncRef/Closure）按 id 相等、无稳定地址，保持弱贡献（collision 合法，见 eq 侧）。
            Value::Object(gc) => crate::corelib::core::object::identity_hash(
                crate::object::ObjRef::as_ptr(&gc) as *const _ as *const u8 as usize) as u32,
            Value::Array(gc) => crate::corelib::core::object::identity_hash(
                crate::object::ArrRef::as_ptr(&gc) as *const _ as *const u8 as usize) as u32,
            _ => 0,
        };
        h = (h ^ rh).wrapping_mul(FNV_PRIME);
    }
    (h & 0x7fff_ffff) as i64
}


#[cfg(test)]
#[path = "boxed_hash_tests.rs"]
mod boxed_hash_tests;
