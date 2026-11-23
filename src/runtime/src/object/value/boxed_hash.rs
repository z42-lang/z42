//! Content hashing for a boxed value-struct payload.

use super::Value;

/// add-struct-object-methods (PR2b): boxed 值 struct 的 GetHashCode——FNV-1a over 基元字节 blob + 混入
/// 引用叶子哈希（string 走内容 FNV；object/array 叶子走地址身份哈希（`identity_hash`——GC 不移动对象，
/// 地址稳定），见 add-value-hash-ref-leaves）。结果 `& 0x7fffffff` 非负（`__str_hash_code` 同款，
/// Dictionary 契约）。值相等的 struct → 字节+refs 相同 → 同哈希（契约满足）。⚠️边角：float ±0.0
/// 字节不同→哈希不同，而 Equals 的浮点 == 判 +0==-0 → 极少数含 ±0 float 的 struct 违反契约
/// （pre-1.0 文档标注，与 C# 历史一致；修约需运行期知道浮点字段位置=zbc 字段表 prim 码，另行推进）。
pub fn struct_hash_code(gc: &crate::object::ObjRef) -> i64 {
    // add-boxed-struct-identity (P4b): read the blob out of the shared box object.
    // The blob's reference slots hold reference words — hash the bytes with them zeroed.
    let (bytes, refs) = gc.borrow().boxed_blob();
    hash_struct_payload(&bytes, &refs)
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
