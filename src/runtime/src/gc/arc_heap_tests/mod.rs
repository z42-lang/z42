//! `ArcMagrGC` 单元测试 —— 覆盖全部 11 个能力组。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::gc::{GcEvent, GcHandleKind, GcKind, GcObserver, MagrGC, ArcMagrGC, SnapshotCoverage};
use crate::metadata::{NativeData, TypeDesc, Value};
use crate::metadata::types::FieldSlot;

/// unify-object-byte-layout (PR-2): a test TypeDesc with 4 **reference-typed** fields
/// (`f0..f3`). The GC tests store arbitrary `Value`s (object refs to test reachability,
/// or primitives) into low slots; reference fields land in `ScriptObject::refs` via the
/// synthesis fallback (`object_regions` / `field_value` see no delivered object layout,
/// so they synthesize from these fields). Tests access refs directly as `obj.refs[i]`,
/// and `alloc_object`'s `slots` vec is applied by index through `set_field_value`.
/// (Pre-PR-2 the descriptor had zero fields and `alloc_object` stored the raw `slots`
/// vec; the byte-layout model sizes storage from the type, so the fields are now real.)
pub(super) fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    let fields: Vec<FieldSlot> = (0..4).map(|i| FieldSlot {
        name: format!("f{i}").into(),
        type_tag: "object".into(),
        visibility: 0,
    }).collect();
    let field_index: crate::metadata::NameIndex =
        fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

mod allocation;
mod collection;
mod concurrent_mark;
mod config_stats;
mod cycle_collection;
mod events;
mod finalization;
mod generational;
// `invariants` calls `ArcMagrGC::debug_validate_invariants()` which is
// `#[cfg(debug_assertions)]` only. Gate the module to match so
// `cargo build --release --lib --tests` doesn't break.
// (fix-gc-tests-release-build 2026-05-27)
#[cfg(debug_assertions)]
mod invariants;
mod mark_phase;
mod mode_selection;
mod multi_vm;
mod object_model;
mod oom;
mod pause_histogram;
mod roots;
mod send_sync;
mod stress;
mod tlab;
mod weak_refs;
mod write_barriers;
