//! Pure name/id and folding facts. Callable ABI entries belong to execution.
use std::{collections::HashMap, sync::OnceLock};
use super::ids::tokens::BuiltinId;

#[derive(Clone, Copy)]
pub struct BuiltinInfo {
    pub name: &'static str,
    pub frameless: bool,
}

/// Immutable provider catalog. A VM's execution table has the same record order.
/// The default facade shares this catalog across VMs; queries never execute providers.
pub struct BuiltinCatalog {
    records: &'static [BuiltinInfo],
    index: OnceLock<HashMap<&'static str, u32>>,
}

impl BuiltinCatalog {
    pub const fn new(records: &'static [BuiltinInfo]) -> Self {
        Self { records, index: OnceLock::new() }
    }

    pub fn id_of(&self, name: &str) -> Option<BuiltinId> {
        self.index.get_or_init(|| self.records.iter().enumerate()
            .map(|(i, info)| (info.name, i as u32)).collect())
            .get(name).copied().map(BuiltinId)
    }

    pub(crate) fn folds_frameless(&self, id: u32) -> bool {
        self.records.get(id as usize).is_some_and(|info| info.frameless)
    }

    pub fn name<'a>(&self, id: u32, fallback: &'a str) -> &'a str {
        self.records.get(id as usize).map_or(fallback, |info| info.name)
    }
}
