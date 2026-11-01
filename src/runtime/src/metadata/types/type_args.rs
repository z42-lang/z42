//! 泛型实例类型实参的**驻留句柄**（与 [`super::ElemType`] 同一套路）。
//!
//! 每个带实参的泛型实例都要记住构造时的类型实参（`default(T)`、`GetType().GetGenericArguments()`、
//! `__class_type_arg`、型参字段的零值都读它）。此前是每个对象一个 `ObjExtras` 盒子装
//! `Box<[String]>`：闭合 `Box<int>` 一个对象就多三次 malloc（盒子、切片、`"int"`），约 55 B。
//! 而实参列表是**有限集**（已加载代码里 `ObjNew` 指令带的实参、实例化描述符名字里解析出的实参，
//! 外加少量反射构造的实例化），于是进程级驻留一次、永不释放：
//!
//! - [`TypeArgs`] = 指向驻留条目的 **8 B 细指针**，`Copy`，同内容恒同址；
//! - 条目持有 `Box<[String]>`，[`TypeArgs::as_slice`] 给出 `&'static [String]`，
//!   所有读者照旧拿 `&[String]`，可见行为逐字不变；
//! - 驻留在 VM 函数站点侧表首次建立或类型注册时完成；IR 只保存名称，
//!   建对象的热路径只拷一个指针，不查表、不哈希。
//!
//! 进程级而非 VM 级：包按需惰性加载、多个 VM 可以并存，句柄要在它们之间共享（同 `ElemType`）。
//! 擦除代码里的 `new Node<T>()` 实参是字面 `"T"`，也照样驻留成一个条目，所有这类实例共用它。

use std::sync::{OnceLock, RwLock};

use rustc_hash::FxHashMap;

use super::ElemKind;

/// 一个驻留条目（进程生命期，泄漏）。对齐 ≥ 8，`ScriptObject` 的元信息字借它的低位做标签。
#[derive(Debug)]
pub struct TypeArgsInfo {
    args: Box<[String]>,
    /// M13: 每个实参名按 [`ElemKind::of_type_param`] 折好的打包种类（基元宽度或 `Mixed`），
    /// 驻留时算一次——`new T[n]` 的分配点读它，不重复字符串 match。
    kinds: Box<[ElemKind]>,
    /// 实参里有没有型参标记（`$mta:N` / `$cta:N@Owner`，见 `metadata::index::type_param_markers`）——
    /// 共享泛型体里 `new List<T>()` 的站点实参。驻留时判一次，建对象的热路径只读这一位。
    open: bool,
}

/// 驻留的类型实参列表：8 B 细指针，`Copy`。`Deref<Target = [String]>`，所以既有的
/// `.is_empty()` / `.iter()` / `.get(i)` / `&ta[..]` 用法照旧。驻留保证同内容同址，
/// 相等直接比指针。
#[derive(Clone, Copy)]
pub struct TypeArgs(&'static TypeArgsInfo);

fn table() -> &'static RwLock<FxHashMap<&'static [String], TypeArgs>> {
    static T: OnceLock<RwLock<FxHashMap<&'static [String], TypeArgs>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(FxHashMap::default()))
}

impl TypeArgs {
    /// 驻留 `args`，返回其句柄（同内容恒同址）。命中只拿读锁；首见才分配并泄漏一个条目。
    /// 空列表走 [`Self::empty`]，不碰表。
    pub fn intern(args: &[String]) -> Self {
        if args.is_empty() {
            return Self::empty();
        }
        if let Some(t) = table().read().unwrap_or_else(|e| e.into_inner()).get(args) {
            return *t;
        }
        let mut w = table().write().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = w.get(args) {
            return *t;
        }
        let info: &'static TypeArgsInfo = Box::leak(Box::new(TypeArgsInfo {
            args: args.to_vec().into_boxed_slice(),
            kinds: args.iter().map(|n| ElemKind::of_type_param(n)).collect(),
            open: args.iter().any(|n| crate::metadata::index::type_param_markers::has_markers(n)),
        }));
        // `info` is leaked ⇒ its slice lives for the process, so a `&'static [String]`
        // view of it is sound as the map key.
        let key: &'static [String] = &info.args;
        let t = TypeArgs(info);
        w.insert(key, t);
        t
    }

    /// 空实参列表（非泛型 / 未实例化）。
    pub fn empty() -> Self {
        static E: OnceLock<TypeArgs> = OnceLock::new();
        *E.get_or_init(|| {
            TypeArgs(Box::leak(Box::new(TypeArgsInfo {
                args: Box::new([]),
                kinds: Box::new([]),
                open: false,
            })))
        })
    }

    #[inline]
    pub fn as_slice(self) -> &'static [String] {
        &self.0.args
    }

    /// 第 `i` 个实参的打包种类（M13;越界 → `Mixed`）。驻留时算好,分配点一次数组读。
    #[inline]
    pub fn elem_kind(self, i: usize) -> ElemKind {
        self.0.kinds.get(i).copied().unwrap_or(ElemKind::Mixed)
    }

    /// 是否含型参标记（站点实参要按运行期型参代换，见 `interp::type_param_env`）。
    #[inline]
    pub fn has_markers(self) -> bool {
        self.0.open
    }

    /// JIT 把句柄当一个指针常量烤进代码；helper 用 [`Self::from_raw`] 还原。
    #[inline]
    pub fn as_raw(self) -> *const TypeArgsInfo {
        self.0
    }

    /// # Safety
    /// `p` 必须来自 [`Self::as_raw`]（驻留条目泄漏、永不释放）。
    #[inline]
    pub unsafe fn from_raw(p: *const TypeArgsInfo) -> Self {
        // SAFETY: caller contract — `p` came from `as_raw` on a leaked `'static` entry.
        TypeArgs(unsafe { &*p })
    }
}

impl Default for TypeArgs {
    #[inline]
    fn default() -> Self {
        Self::empty()
    }
}

impl std::ops::Deref for TypeArgs {
    type Target = [String];
    #[inline]
    fn deref(&self) -> &[String] {
        &self.0.args
    }
}

impl PartialEq for TypeArgs {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // 驻留 ⇒ 同内容同址；`empty()` 不进表，但空列表之间按内容也相等。
        std::ptr::eq(self.0, other.0) || (self.0.args.is_empty() && other.0.args.is_empty())
    }
}
impl Eq for TypeArgs {}

impl std::fmt::Debug for TypeArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_slice(), f)
    }
}

#[cfg(test)]
#[path = "type_args_tests.rs"]
mod type_args_tests;
