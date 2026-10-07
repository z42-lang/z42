//! perf-array-elem-type-intern：数组元素类型名的**驻留句柄**。
//!
//! 每个 `ArrayObj` 都要记住元素类型的 FQ 名（`arr.GetType().GetElementType()` 不擦除），
//! 此前是每个数组一份 `Arc::from(element_type)` —— 建一个 `long[8]` 就多一次 ~24 B 的
//! malloc，外加头里 16 B 的胖指针。元素类型名是**有限集**（来自已加载代码的 `ArrayNew` /
//! `ArrayNewLit` 指令，加上少量反射 / 内建名），于是进程级驻留一次、永不释放：
//!
//! - [`ElemType`] = 指向驻留条目的 **8 B 细指针**，`Copy`，建数组零分配、零原子操作；
//! - 条目缓存 [`ElemKind`]（选哪种 backing），建数组不再逐次 `match` 字符串；
//! - 条目同时持有一份 `Arc<str>`，需要 `Arc<str>` 的旁路（struct 元素拷出到 arena）
//!   只做一次引用计数 +1，与此前一致。
//!
//! 指令解码时（`instr_decode.rs`）就把名字驻留进 `ArrayNewInsn::element_type`，所以
//! interp / JIT 的建数组热路径连查表都没有；其余 Rust 侧调用点走 [`ElemType::intern`]
//! （读锁 + FxHash 查表）。

use std::sync::{Arc, OnceLock, RwLock};

use rustc_hash::FxHashMap;

use super::ElemKind;

/// 一个驻留条目（进程生命期，泄漏）。
#[derive(Debug)]
pub struct ElemTypeInfo {
    name: Arc<str>,
    kind: ElemKind,
}

/// 驻留的数组元素类型名：8 B 细指针，`Copy`。`Deref<Target = str>`，所以既有的
/// `&*arr.element_type` / `elem_tag(&a.element_type)` / `.to_string()` 用法照旧。
/// 相等按名字比（驻留保证同名同址，比较先走指针快路）。
#[derive(Clone, Copy)]
pub struct ElemType(&'static ElemTypeInfo);

fn table() -> &'static RwLock<FxHashMap<&'static str, ElemType>> {
    static T: OnceLock<RwLock<FxHashMap<&'static str, ElemType>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(FxHashMap::default()))
}

impl ElemType {
    /// 驻留 `name`，返回其句柄（同名恒同址）。命中只拿读锁；首见才分配并泄漏一个条目。
    pub fn intern(name: &str) -> Self {
        if let Some(t) = table().read().unwrap_or_else(|e| e.into_inner()).get(name) {
            return *t;
        }
        let mut w = table().write().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = w.get(name) {
            return *t;
        }
        let info: &'static ElemTypeInfo =
            Box::leak(Box::new(ElemTypeInfo { name: Arc::from(name), kind: ElemKind::of(name) }));
        // SAFETY-free: `info` is leaked ⇒ its `name` buffer lives for the process, so a
        // `&'static str` view of it is sound as the map key.
        let key: &'static str = &info.name;
        let t = ElemType(info);
        w.insert(key, t);
        t
    }

    /// 元素类型未知（Rust 合成的数组，如反射结果集）。
    pub fn empty() -> Self {
        static E: OnceLock<ElemType> = OnceLock::new();
        *E.get_or_init(|| Self::intern(""))
    }

    /// `byte`（FFI 返回 / `from_bytes`）。
    pub fn byte() -> Self {
        static B: OnceLock<ElemType> = OnceLock::new();
        *B.get_or_init(|| Self::intern("byte"))
    }

    /// `char`（`ToCharArray`）。
    pub fn char() -> Self {
        static C: OnceLock<ElemType> = OnceLock::new();
        *C.get_or_init(|| Self::intern("char"))
    }

    #[inline]
    pub fn as_str(self) -> &'static str {
        &self.0.name
    }

    /// 这个元素类型选哪种 backing（驻留时算好）。
    #[inline]
    pub fn kind(self) -> ElemKind {
        self.0.kind
    }

    /// 名字的 `Arc<str>`（引用计数 +1，不分配）。
    #[inline]
    pub fn arc(self) -> Arc<str> {
        self.0.name.clone()
    }

    /// JIT 把句柄当一个指针常量烤进代码；helper 用 [`Self::from_raw`] 还原。
    #[inline]
    pub fn as_raw(self) -> *const ElemTypeInfo {
        self.0
    }

    /// # Safety
    /// `p` 必须来自 [`Self::as_raw`]（驻留条目泄漏、永不释放）。
    #[inline]
    pub unsafe fn from_raw(p: *const ElemTypeInfo) -> Self {
        // SAFETY: caller contract — `p` came from `as_raw` on a leaked `'static` entry.
        ElemType(unsafe { &*p })
    }
}

impl std::ops::Deref for ElemType {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        &self.0.name
    }
}

impl PartialEq for ElemType {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0) || self.0.name == other.0.name
    }
}
impl Eq for ElemType {}

impl std::fmt::Debug for ElemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for ElemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_is_idempotent_and_thin() {
        let a = ElemType::intern("geometry.Point");
        let b = ElemType::intern(&String::from("geometry.Point"));
        assert!(std::ptr::eq(a.as_raw(), b.as_raw()), "same name ⇒ same entry");
        assert_eq!(&*a, "geometry.Point");
        assert_eq!(a.kind(), ElemKind::Boxed);
        assert_eq!(ElemType::intern("int").kind(), ElemKind::I32);
        assert_eq!(ElemType::byte().kind(), ElemKind::Bytes);
        assert_eq!(&*ElemType::empty(), "");
        assert_eq!(std::mem::size_of::<ElemType>(), 8);
        // SAFETY: round-trips a pointer obtained from `as_raw`.
        let c = unsafe { ElemType::from_raw(a.as_raw()) };
        assert_eq!(c, a);
    }
}
