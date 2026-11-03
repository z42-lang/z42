//! Linked entry module and identity tables owned by one VM.
use std::sync::Arc;
use super::{Module, ids::{func_table::FuncTable, type_table::TypeTable, str_table::StrTable}};

pub(crate) struct RuntimeState {
    /// Linked entry projection; its immutable package data can be shared.
    pub(crate) module: Option<Arc<Module>>,
    /// Per-VM append-only FnId identity and function ownership.
    pub(crate) funcs: Arc<FuncTable>,
    /// This VM's sparse view of process-global TypeId identities.
    pub(crate) types: Arc<TypeTable>,
    /// Per-VM literal identities and interned heap strings (GC roots).
    pub(crate) strings: Arc<StrTable>,
}

impl RuntimeState {
    pub(crate) fn new(module: Option<Arc<Module>>) -> Self {
        // Each VM gets its own tables; immutable package data remains shared by the module.
        let funcs = Arc::new(FuncTable::new(module.clone()));
        let types = Arc::new(TypeTable::new(module.as_deref()));
        let strings = Arc::new(StrTable::new(module.clone()));
        Self { module, funcs, types, strings }
    }
}
