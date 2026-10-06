/// Module merge logic: combine multiple IR modules into one for .zpkg loading.
///
/// The only complexity is the `string_pool` offset remap: each `ConstStr { dst, idx }`
/// instruction references an index into its module's string pool. When N pools are
/// concatenated, every index from module i must be shifted by the cumulative length of
/// pools 0..i-1.
use super::bytecode::{BasicBlock, Instruction, Module};
use anyhow::Result;
use std::collections::HashSet;

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
        return Ok(modules.into_iter().next().unwrap());
    }

    let name = modules[0].name.clone();
    let mut string_pool: Vec<String> = Vec::new();
    let mut seen_classes: HashSet<String> = HashSet::new();
    let mut classes = Vec::new();
    let mut seen_functions: HashSet<String> = HashSet::new();
    let mut functions = Vec::new();

    for mut module in modules {
        let str_offset = string_pool.len() as u32;
        string_pool.extend(module.string_pool);

        // Idempotent class merge: keep first occurrence by name.
        for cls in module.classes {
            if seen_classes.insert(cls.name.clone()) {
                classes.push(cls);
            }
        }

        remap_functions(&mut module.functions, str_offset);

        // Idempotent function merge: keep first occurrence by name.
        for func in module.functions {
            if seen_functions.insert(func.name.clone()) {
                functions.push(func);
            }
        }
    }

    let merged = Module {
        name, string_pool, classes, functions,
        type_registry: rustc_hash::FxHashMap::default(),
        func_index: rustc_hash::FxHashMap::default(),
    };
    Ok(merged)
}

/// Shift every `ConstStr.idx` by `str_offset` so cross-module merge produces
/// a flat global string index space.
fn remap_functions(functions: &mut Vec<super::bytecode::Function>, str_offset: u32) {
    if str_offset == 0 {
        return; // nothing to do
    }
    for func in functions.iter_mut() {
        for block in func.blocks.iter_mut() {
            remap_block(block, str_offset);
        }
    }
}

fn remap_block(block: &mut BasicBlock, str_offset: u32) {
    for instr in block.instructions.iter_mut() {
        if let Instruction::ConstStr { idx, .. } = instr {
            *idx += str_offset;
        }
    }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
