//! Content hashing for a boxed value-struct payload.

use super::Value;

/// Canonicalize negative zero in the float lanes of a value snapshot. Struct equality
/// treats both zeros as equal, so the default hash must see the same bytes for both.
pub fn normalize_signed_zero(
    bytes: &mut [u8], float_fields: &[std::ops::Range<usize>],
) -> anyhow::Result<()> {
    for range in float_fields {
        let field = bytes.get_mut(range.clone())
            .ok_or_else(|| anyhow::anyhow!("struct hash: invalid float field range {range:?}"))?;
        if let Some((sign, rest)) = field.split_last_mut() {
            if *sign == 0x80 && rest.iter().all(|&b| b == 0) {
                *sign = 0;
            }
        }
    }
    Ok(())
}

/// Hash borrowed arena storage. A float layout uses a scratch copy for zero normalization.
#[inline]
pub fn hash_struct_with_floats(
    bytes: &[u8], refs: &[Value], float_fields: &[std::ops::Range<usize>],
) -> anyhow::Result<i64> {
    if float_fields.is_empty() {
        return Ok(hash_struct_payload(bytes, refs));
    }
    let mut copy = bytes.to_vec();
    normalize_signed_zero(&mut copy, float_fields)?;
    Ok(hash_struct_payload(&copy, refs))
}

/// add-struct-value-eq-fastpath：payload 级哈希——boxed、frame arena `StructRef`、
/// `struct[]` 元素三种表示共用同一实现，同值 struct 无论从哪种表示进来哈希都相同
/// （值相等 ⇒ 哈希相等这条契约跨表示成立）。
pub fn hash_struct_payload(bytes: &[u8], refs: &[Value]) -> i64 {
    let mut h: u32 = 2_166_136_261;
    for &byte in bytes.iter() { h ^= byte as u32; h = h.wrapping_mul(16_777_619); }
    for r in refs.iter() {
        let rh: u32 = match r {
            Value::Str(s) => {
                let mut sh: u32 = 2_166_136_261;
                for &x in s.as_bytes() { sh ^= x as u32; sh = sh.wrapping_mul(16_777_619); }
                sh
            }
            // add-value-hash-ref-leaves：object/array 叶子此前贡献常量 0——Equals 按引用身份比较，
            // 哈希却全同 ⇒ 引用叶子互异的 struct 全部落进同一个桶（O(n) 探测链）。改贡献地址身份
            // 哈希（`identity_hash`，fmix64 混合；GC 不移动对象故稳定）：身份相等 ⇒ 哈希相等（契约
            // 成立），不同身份 → 良好散布。null/委托叶子仍贡献常量：null 相等语义下合法；委托
            // （FuncRef/Closure）按 id 相等、无稳定地址，保持弱贡献（collision 合法，见 eq 侧）。
            Value::Object(gc) => crate::corelib::core::object::identity_hash(
                crate::object::ObjRef::as_ptr(gc) as *const _ as *const u8 as usize) as u32,
            Value::Array(gc) => crate::corelib::core::object::identity_hash(
                crate::object::ArrRef::as_ptr(gc) as *const _ as *const u8 as usize) as u32,
            _ => 0,
        };
        h ^= rh; h = h.wrapping_mul(16_777_619);
    }
    (h & 0x7fff_ffff) as i64
}


#[cfg(test)]
#[path = "boxed_hash_tests.rs"]
mod boxed_hash_tests;
