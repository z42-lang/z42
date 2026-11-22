//! Registered function identity and signature queries; never load or execute.
use crate::metadata::{Function, Module, ids::func_table::FuncTable};

/// P1-2: the callee a bound `Call` token or a `VCall` PIC payload names. In range of
/// `module.functions` it is that index (for the entry module that *is* the `FnId`); past
/// it, a lazily loaded function's `FnId` (only ever stored when `module` is the
/// `FuncTable`'s entry module, see `ResolvedTokens::method_tokens`). Lock-free either
/// way. `None` cannot happen for an id a call path stored once the storing thread's
/// registration is visible; callers then fall back to binding by name.
///
/// M3 single-arm-per-track: for the entry module the token is a global `FnId`
/// and the entry's own functions live in the table's slots (`0..entry_len`,
/// pointers into the module the table keeps alive — see `FuncTable::new`), so
/// ONE slots lookup covers both cross-package ids and intra-entry calls; the
/// previous `module.functions` fast arm made every cross-package call pay a
/// doomed bounds check first (+21% on `list_count_repeated`). A non-entry
/// module (unit tests with a bare context) keeps its LOCAL index space.
#[inline]
pub fn fn_by_id<'m>(funcs: &'m FuncTable, module: &'m Module, id: u32) -> Option<&'m Function> {
    if funcs.is_entry(module) {
        funcs.get(crate::metadata::ids::tokens::FnId(id))
    } else {
        module.functions.get(id as usize)
    }
}

/// The callee's JIT id / `FnId` when the call path may cache it for `module`: a
/// lazily loaded function only counts when `module` is the `FuncTable`'s entry
/// module and the table holds exactly this function under the id (see
/// `ResolvedTokens::method_tokens`).
#[inline]
pub fn lazy_call_id(funcs: &FuncTable, module: &Module, f: &Function) -> Option<usize> {
    let id = f.id.get()?;
    (funcs.is_entry(module) && funcs.get(id).is_some_and(|g| std::ptr::eq(g, f))).then_some(id.0 as usize)
}

/// 被调函数可接受的**物理**实参数区间：含 `this`（实例方法 / 构造器的 `param_count` 本就含它）、
/// 含 sret 隐藏返回槽。`max == u16::MAX` ⇒ `params` 变长，无上界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallArity {
    pub min: u16,
    pub max: u16,
}

impl CallArity {
    /// 调用方传入 `phys` 个值（含 `this`、含 sret 槽）时是否可接受。
    #[inline]
    pub fn accepts(&self, phys: usize) -> bool {
        let phys = phys.min(u16::MAX as usize) as u16;
        phys >= self.min && phys <= self.max
    }
}

/// Physical arguments include this and sret; defaults are filled by the compiler.
pub fn call_arity(f: &crate::metadata::Function) -> CallArity {
    let sret = (f.method_flags & crate::zpkg::ir::METHOD_FLAG_SRET) != 0;
    let expected = f.param_count.saturating_add(sret as usize).min(u16::MAX as usize) as u16;
    let max = if f.params_from != 0xFF { u16::MAX } else { expected };
    CallArity { min: expected, max }
}
