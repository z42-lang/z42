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
    records: std::borrow::Cow<'static, [BuiltinInfo]>,
    reserved: Vec<&'static str>,
    index: OnceLock<HashMap<&'static str, Option<u32>>>,
}

impl BuiltinCatalog {
    pub const fn new(records: &'static [BuiltinInfo]) -> Self {
        Self { records: std::borrow::Cow::Borrowed(records), reserved: Vec::new(), index: OnceLock::new() }
    }

    pub fn from_records(records: Vec<BuiltinInfo>) -> Self {
        Self { records: std::borrow::Cow::Owned(records), reserved: Vec::new(), index: OnceLock::new() }
    }

    pub fn with_reserved_names(mut self, names: Vec<&'static str>) -> Self { self.reserved = names; self.index = OnceLock::new(); self }

    pub fn id_of(&self, name: &str) -> Option<BuiltinId> {
        self.index.get_or_init(|| {
            let mut names: HashMap<_, _> = self.reserved.iter().map(|&name| (name, None)).collect();
            for (index, info) in self.records.iter().enumerate() {
                // Ambiguous bare names never acquire a load-order-dependent winner.
                names.entry(info.name).and_modify(|id| *id = None).or_insert(Some(index as u32));
            }
            names
        }).get(name).copied().flatten().map(BuiltinId)
    }

    pub(crate) fn folds_frameless(&self, id: u32) -> bool {
        self.records.get(id as usize).is_some_and(|info| info.frameless)
    }

    pub fn name<'a>(&self, id: u32, fallback: &'a str) -> &'a str {
        self.records.get(id as usize).map_or(fallback, |info| info.name)
    }
}
