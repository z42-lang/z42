/// Module merge logic: combine multiple IR modules into one for .zpkg loading.
///
/// The only complexity is the `string_pool` offset remap: each `ConstStr { dst, idx }`
/// instruction references an index into its module's string pool. When N pools are
/// concatenated, every index from module i must be shifted by the cumulative length of
/// pools 0..i-1.
use crate::vm::runtime::Module;
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

    for module in modules {
        let (module, mut module_functions, _) = module.into_parts();
        let str_offset = string_pool.len() as u32;
        string_pool.extend(module.string_pool);

        // Idempotent class merge: keep first occurrence by name.
        for cls in module.classes {
            if seen_classes.insert(cls.name.clone()) {
                classes.push(cls);
            }
        }

        remap_functions(&mut module_functions, str_offset);

        // Idempotent function merge: keep first occurrence by name.
        for func in module_functions {
            if seen_functions.insert(func.name.clone()) {
                functions.push(func);
            }
        }
    }

    let merged = Module::new(crate::metadata::image::ModuleData {
        name,
        string_pool,
        classes,
        func_index: rustc_hash::FxHashMap::default(),
    }, functions, rustc_hash::FxHashMap::default());
    Ok(merged)
}

/// Shift every `ConstStr.idx` by `str_offset` so cross-module merge produces
/// a flat global string index space.
fn remap_functions(functions: &mut [crate::vm::runtime::Function], str_offset: u32) {
    for func in functions { func.remap_string_ids(str_offset); }
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
