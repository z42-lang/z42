//! Heap inspection result contracts; algorithms live in the profiler component.
use crate::gc::walk::RootKind;
/// Whether a heap referrer is a script object or an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RetainerKind {
    Object = 0,
    Array = 1,
}

/// A direct heap referrer of the target (an object/array that holds a reference).
#[derive(Debug, Clone)]
pub struct RetainerInfo {
    pub kind: RetainerKind,
    /// FQ type name of the referrer (e.g. `"Demo.Holder"`, `"int[]"`).
    pub type_name: String,
    /// The referrer's heap identity (data ptr as usize) — dedup + stable id.
    pub id: usize,
}

/// A GC root that retains the target (category-level).
#[derive(Debug, Clone)]
pub struct RootInfo {
    pub kind: RootKind,
}
