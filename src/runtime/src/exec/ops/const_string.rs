//! Materialize literal metadata into GC-managed strings for execution.
use crate::{vm::VmContext, metadata::Module, object::string::vstr::Str};
#[inline]
pub fn load(ctx: &VmContext, module: &Module, idx: u32) -> Option<Str> {
    let strings = &ctx.core().runtime().strings;
    if strings.is_entry(module) {
        return strings.get_or_intern(idx, |text| ctx.heap().alloc_str(text));
    }
    load_foreign(ctx, module, idx)
}
#[cold]
fn load_foreign(ctx: &VmContext, module: &Module, idx: u32) -> Option<Str> {
    if let Some(text) = module.string_pool.get(idx as usize) { return Some(ctx.heap().alloc_str(text)); }
    ctx.core().runtime().strings.get_or_intern(idx, |text| ctx.heap().alloc_str(text))
}
