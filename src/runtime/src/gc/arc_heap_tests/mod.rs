//! `ArcMagrGC` 单元测试 —— 覆盖全部 11 个能力组。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::gc::{GcEvent, GcHandleKind, GcKind, GcObserver, MagrGC, ArcMagrGC, SnapshotCoverage};
use crate::metadata::{NativeData, TypeDesc, Value};

pub(super) fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
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
mod weak_refs;
mod write_barriers;
