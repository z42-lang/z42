//! Linked identities, static storage and initialization facts owned by one VM.
use std::sync::Arc;
use super::{Module, ids::{func_table::FuncTable, type_table::TypeTable, str_table::StrTable}};

pub struct RuntimeState {
    pub builtins: &'static super::builtins::BuiltinCatalog,
    pub statics: super::statics::StaticTable,
    pub cctors: Arc<super::statics::CctorRegistry>,
    pub static_init_error: parking_lot::Mutex<Option<String>>,
    /// Linked entry projection; its immutable package data can be shared.
    pub module: Option<Arc<Module>>,
    /// Per-VM append-only FnId identity and function ownership.
    pub funcs: Arc<FuncTable>,
    /// This VM's sparse view of process-global TypeId identities.
    pub types: Arc<TypeTable>,
    /// Per-VM literal identities and interned heap strings (GC roots).
    pub strings: Arc<StrTable>,
    pub reflection_types: Box<super::reflection_types::ReflectionTypes>,
}

impl RuntimeState {
    pub fn new(module: Option<Arc<Module>>, builtins: &'static super::builtins::BuiltinCatalog) -> Self {
        // Each VM gets its own tables; immutable package data remains shared by the module.
        let funcs = Arc::new(FuncTable::new(module.clone()));
        let types = Arc::new(TypeTable::new(module.as_deref()));
        let strings = Arc::new(StrTable::new(module.clone()));
        Self {
            builtins,
            statics: Default::default(),
            cctors: Arc::new(super::statics::CctorRegistry::default()),
            static_init_error: parking_lot::Mutex::new(None),
            module, funcs, types, strings,
            reflection_types: Box::default(),
        }
    }
}

/// Per-thread front caches contain only complete, stable lookup answers.
/// Explicit module reload clears all four together; no cache owns its context.
pub struct ThreadRuntimeState {
    pub subclass_memo: parking_lot::Mutex<rustc_hash::FxHashMap<u64, bool>>,
    pub isa_cache: super::dispatch::isa_cache::IsaCache,
    pub type_lookup_cache: parking_lot::Mutex<rustc_hash::FxHashMap<Box<str>, Arc<crate::object::TypeDesc>>>,
    pub fn_lookup_cache: parking_lot::Mutex<rustc_hash::FxHashMap<Box<str>, Arc<super::Function>>>,
}

impl ThreadRuntimeState {
    pub fn new() -> Self {
        Self {
            subclass_memo: Default::default(),
            isa_cache: super::dispatch::isa_cache::IsaCache::new(),
            type_lookup_cache: Default::default(),
            fn_lookup_cache: Default::default(),
        }
    }
}
