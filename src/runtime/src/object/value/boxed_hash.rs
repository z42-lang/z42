//! Content hashing for a boxed value-struct payload.

use super::Value;

/// add-struct-object-methods (PR2b): boxed 值 struct 的 GetHashCode——FNV-1a over 基元字节 blob + 混入
/// 引用叶子哈希（string 走内容 FNV；object/array 叶子暂贡献常量 = 弱但合法，因 Equals 对引用叶子按引用比较，
/// 不同引用→不 Equals→哈希可碰撞）。结果 `& 0x7fffffff` 非负（`__str_hash_code` 同款，Dictionary 契约）。
/// 值相等的 struct → 字节+refs 相同 → 同哈希（契约满足）。⚠️边角：float ±0.0 字节不同→哈希不同，而 Equals
/// 的浮点 == 判 +0==-0 → 极少数含 ±0 float 的 struct 违反契约（pre-1.0 文档标注，与 C# 历史一致）。
pub(crate) fn struct_hash_code(gc: &crate::object::ObjRef) -> i64 {
    // add-boxed-struct-identity (P4b): read the blob out of the shared box object.
    // The blob's reference slots hold reference words — hash the bytes with them zeroed.
    let (bytes, refs) = gc.borrow().boxed_blob();
    let mut h: u32 = 2_166_136_261;
    for &byte in bytes.iter() { h ^= byte as u32; h = h.wrapping_mul(16_777_619); }
    for r in refs.iter() {
        let rh: u32 = match r {
            Value::Str(s) => {
                let mut sh: u32 = 2_166_136_261;
                for &x in s.as_bytes() { sh ^= x as u32; sh = sh.wrapping_mul(16_777_619); }
                sh
            }
            _ => 0,   // object/array/null 叶子：弱贡献（引用相等语义下 collisions 合法）
        };
        h ^= rh; h = h.wrapping_mul(16_777_619);
    }
    (h & 0x7fff_ffff) as i64
}

