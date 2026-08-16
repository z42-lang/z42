/// Module merge logic: combine multiple IR modules into one for .zpkg loading.
///
/// The only complexity is the `string_pool` offset remap: each `ConstStr { dst, idx }`
/// instruction references an index into its module's string pool. When N pools are
/// concatenated, every index from module i must be shifted by the cumulative length of
/// pools 0..i-1.
use super::bytecode::{BasicBlock, Instruction, Module};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Merge an ordered sequence of IR modules into a single flat module.
///
/// Merge rules:
/// - `string_pool`: concatenated in order; `ConstStr.idx` remapped accordingly.
/// - `classes`: idempotent merge by `name` (last definition wins).
/// - `functions`: idempotent merge by `name` (last definition wins).
/// - `name`: taken from the first module.
pub fn merge_modules(modules: Vec<Module>) -> Result<Module> {
    if modules.is_empty() {
        anyhow::bail!("merge_modules: no modules provided");
    }
    if modules.len() == 1 {
        // Fast path: nothing to merge.
        let mut m = modules.into_iter().next().unwrap();
        // Canonicalise: ensure no stale indices (no-op remap with offset 0)
        remap_functions(&mut m.functions, 0, 0);
        return Ok(m);
    }

    let name = modules[0].name.clone();
    let mut string_pool: Vec<String> = Vec::new();
    let mut seen_classes: HashSet<String> = HashSet::new();
    let mut classes = Vec::new();
    let mut seen_functions: HashSet<String> = HashSet::new();
    let mut functions = Vec::new();
    // 2026-05-02 add-method-group-conversion (D1b): cumulative FuncRef cache
    // slot count across merged modules; LoadFnCached.slot_id 在每个 module 内
    // 局部分配，merge 时按 cumulative offset 重映射到全局 index space。
    let mut func_ref_slot_total: u32 = 0;

    for mut module in modules {
        let str_offset = string_pool.len() as u32;
        let slot_offset = func_ref_slot_total;
        func_ref_slot_total += module.func_ref_cache_slots;
        string_pool.extend(module.string_pool);

        // Idempotent class merge: keep first occurrence by name.
        for cls in module.classes {
            if seen_classes.insert(cls.name.clone()) {
                classes.push(cls);
            }
        }

        remap_functions(&mut module.functions, str_offset, slot_offset);

        // Idempotent function merge: keep first occurrence by name.
        for func in module.functions {
            if seen_functions.insert(func.name.clone()) {
                functions.push(func);
            }
        }
    }

    let merged = Module {
        name, string_pool, classes, functions,
        type_registry: HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index: HashMap::new(),
        func_ref_cache_slots: func_ref_slot_total,
    };
    Ok(merged)
}

/// Shift every `ConstStr.idx` and `LoadFnCached.slot_id` by their respective
/// offsets so cross-module merge produces a flat global index space.
fn remap_functions(
    functions: &mut Vec<super::bytecode::Function>,
    str_offset: u32, slot_offset: u32)
{
    if str_offset == 0 && slot_offset == 0 {
        return; // nothing to do
    }
    for func in functions.iter_mut() {
        for block in func.blocks.iter_mut() {
            remap_block(block, str_offset, slot_offset);
        }
    }
}

fn remap_block(block: &mut BasicBlock, str_offset: u32, slot_offset: u32) {
    for instr in block.instructions.iter_mut() {
        match instr {
            Instruction::ConstStr { idx, .. } if str_offset != 0 => *idx += str_offset,
            Instruction::LoadFnCached(insn) if slot_offset != 0 => insn.slot_id += slot_offset,
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
