//! Module（zbc 模块：函数表 / 类表 / 字符串池 / 类型注册表）。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
use crate::metadata::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Top-level bytecode module.
/// Loaded from `.zbc` binary (or legacy `.z42ir.json`).
#[derive(Debug, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub string_pool: Vec<String>,
    #[serde(default)]
    pub classes: Vec<ClassDesc>,
    pub functions: Vec<Function>,
    /// Pre-built type descriptor registry — populated by the loader after
    /// deserialisation, not stored on disk.  Maps fully-qualified class name
    /// to the corresponding `TypeDesc` (field layout + vtable).
    #[serde(skip)]
    pub type_registry: FxHashMap<String, Arc<TypeDesc>>,
    /// Phase 3 S1 (`tokenize-ir-and-zbc-bump`, 2026-05-09): parallel
    /// by-`TypeId` view of the type registry. Index `i` holds the `Arc<TypeDesc>`
    /// whose `id == TypeId(i as u32)`. Built alongside `type_registry` by
    /// `build_type_registry` (intra-module classes) and extended by
    /// `register_lazy_type` (cross-zpkg lazy load).
    ///
    /// In S1 this is observability infrastructure — consumers still go
    /// through `type_registry` (HashMap by-name). S4 will switch hot paths
    /// to `type_by_id()` once IR fields are tokenised.
    #[serde(skip)]
    pub type_registry_vec: Vec<Arc<TypeDesc>>,
    /// Pre-built function name → index mapping for O(1) call dispatch.
    /// Populated by the loader after deserialisation.
    #[serde(skip)]
    pub func_index: FxHashMap<String, usize>,
    /// 2026-05-02 add-method-group-conversion (D1b): number of FuncRef cache
    /// slots required by `LoadFnCached` instructions. VM allocates a parallel
    /// `Vec<Value>` of this size on `VmContext` at module load.
    #[serde(default)]
    pub func_ref_cache_slots: u32,
}

impl Module {
    /// Phase 3 S1 (`tokenize-ir-and-zbc-bump`, 2026-05-09): O(1) by-`TypeId`
    /// type lookup. Invariant: `type_registry_vec[id.0] == registry[name]`
    /// where `name` is the FQ class name of that TypeDesc, maintained by
    /// `loader::build_type_registry` and `Module::register_lazy_type`.
    #[inline]
    pub fn type_by_id(&self, id: TypeId) -> Option<&Arc<TypeDesc>> {
        if !id.is_resolved() { return None; }
        self.type_registry_vec.get(id.0 as usize)
    }

    /// Append a lazily-loaded TypeDesc to both views (Vec and HashMap),
    /// assigning the next available `TypeId.0`. Returns the assigned id.
    /// Used by `lazy_loader` for cross-zpkg type resolution.
    ///
    /// Caller responsibility: the input `Arc<TypeDesc>` may carry its own
    /// `id` (from another module's build_type_registry); this method
    /// **rebuilds** the Arc with the freshly-allocated module-local id so
    /// downstream `td.id` checks remain consistent. If the type is already
    /// present (by-name match), returns the existing id without modification.
    pub fn register_lazy_type(&mut self, td: Arc<TypeDesc>) -> TypeId {
        if let Some(existing) = self.type_registry.get(&td.name) {
            return existing.id;
        }
        let new_id = TypeId(self.type_registry_vec.len() as u32);
        // Rebuild with the new module-local id (TypeDesc.id is a single u32 —
        // cheap to clone the rest by Arc-internals walking).
        let cold = td.cold.as_deref().map(|c| Box::new(crate::metadata::types::TypeDescCold {
            own_fields:             c.own_fields.clone(),
            own_methods:            c.own_methods.clone(),
            own_static_flags:       c.own_static_flags.clone(),
            type_params:            c.type_params.clone(),
            type_args:              c.type_args.clone(),
            type_param_constraints: c.type_param_constraints.clone(),
            custom_attributes:      c.custom_attributes.clone(),
            static_fields:          c.static_fields.clone(),
            field_attributes:       c.field_attributes.clone(),
            interfaces:             c.interfaces.clone(),
            enum_members:           c.enum_members.clone(),
            iface_methods:          c.iface_methods.clone(),
            struct_layout:          c.struct_layout.clone(),
            inline_layout:          c.inline_layout.clone(),  // add-struct-heap-inline (P3b)
            object_layout:          c.object_layout.clone(),  // unify-object-byte-layout (PR-1)
            composed_object_layout: c.composed_object_layout.clone(),  // unify-object-byte-layout (PR-2)
        }));
        let rebuilt = Arc::new(TypeDesc {
            name: td.name.clone(),
            id: new_id,
            base_name: td.base_name.clone(),
            class_flags: td.class_flags,
            visibility: td.visibility,
            fields: td.fields.clone(),
            field_index: td.field_index.clone(),
            vtable: td.vtable.clone(),
            vtable_index: td.vtable_index.clone(),
            cold,
        });
        self.type_registry.insert(rebuilt.name.clone(), rebuilt.clone());
        self.type_registry_vec.push(rebuilt);
        new_id
    }
}
