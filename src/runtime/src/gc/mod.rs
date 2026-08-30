//! Garbage collector —— heap memory management for z42 VM.
//!
//! # 当前状态（至 Phase 3-OOM 完成 2026-04-29，**主功能完整可投产**）
//!
//! - [`MagrGC`] —— GC 抽象 trait，对齐 MMTk porting contract（10 个能力组 / ~30 方法）
//! - [`GcRef<T>`] / [`WeakGcRef<T>`] —— 堆引用不透明句柄（隐藏 backing 实现）
//! - [`ArcMagrGC`] —— 默认后端，`GcRef` backing 是 `Rc<GcAllocation<T>>`（含 finalizer Cell + 自定义 Drop）
//!   + 完整 host-side 嵌入接口（alloc / roots / write barriers / object model /
//!   collection control / heap config / finalization / weak refs / observers /
//!   profiler / stats）
//!
//! # Phase 路线（见 `docs/design/runtime/gc.md` "Phase 路线（持续迭代）" 段）
//!
//! | Phase | 内容 | 状态 |
//! |-------|------|:---:|
//! | 1 | trait + ArcMagrGC + 6 个脚本驱动 callsite 收口 | ✅ |
//! | 1（扩展）| trait 全面对齐 MMTk porting contract（10 能力组）+ host-side 嵌入接口完整 | ✅ |
//! | 1.5 | corelib NativeFn 签名带 `&VmContext` + 剩余 Rc::new 迁移 | ✅ |
//! | 2 | （**跳过**）—— 直奔 Phase 3 mark-sweep | ⏭ |
//! | 3a | `GcRef<T>` 不透明句柄抽象（backing 仍 Rc<RefCell<T>>，行为零变化）| ✅ |
//! | 3b | Heap registry + snapshot/iterate Full coverage | ✅ |
//! | 3c | **Trial-deletion 环回收器** —— `collect_cycles` 真实回收环引用 | ✅ |
//! | 3d | Finalizer 真触发（collect 时调度）+ 内存压力自动 collect | ✅ |
//! | 3d.1 | External root scanner（VmContext static_fields 自动暴露给 GC，修复漏扫 bug）| ✅ |
//! | 3d.2 | `Std.GC.Collect()` / `UsedBytes()` / `ForceCollect()` 暴露给 z42 脚本 + 端到端 golden test | ✅ |
//! | 3e | GcRef backing → `Rc<GcAllocation<T>>` wrapper + Drop 自动触发 finalizer（含纯 Rc Drop 路径）| ✅ |
//! | 3f | interp 栈扫描（FrameGuard RAII 把 frame.regs 暴露给 scanner，修复 transitive bug）| ✅ |
//! | 3f-2 | JIT JitFrame.regs 对接（6 callsites push/pop frame.regs 到 exec_stack）| ✅ |
//! | 3-OOM | strict OOM 模式（trait `set_strict_oom`；启用后 alloc 越限返 Null 不进 registry/stats）| ✅ |
//!
//! 后续可选迭代轨道见
//! [`docs/design/runtime/gc.md`](../../../docs/design/runtime/gc.md)
//! "GC 后续迭代规划" 段：A 性能（自定义 allocator / mark-sweep / generational
//! / concurrent）、B 嵌入式工具（OOM 异常 / 软引用 / heap snapshot 导出 /
//! alloc 站点追踪 / pause 直方图）、C 测试质量（debug invariants / stress 压测）、
//! D MMTk 集成（终极方向）。

pub mod ambient;
pub mod heap;
pub mod arc_heap;
pub mod mode;
pub mod refs;
pub mod region;
pub mod tlab;
pub mod var_region;
pub mod retention;
pub mod safepoint;
pub mod sampler;
pub mod snapshot;
pub mod soft_registry;
pub mod types;

pub use heap::MagrGC;
pub use sampler::Sampler;
pub use arc_heap::ArcMagrGC;
pub use mode::GcMode;
pub use refs::{GcRef, WeakGcRef};
pub use safepoint::{
    check_safepoint, request_gc_pause, GcPauseGuard, GcPhase, NativeParkGuard, NativeUnparkGuard,
};
pub use snapshot::{
    build_graph_snapshot, serialize_v8_heapsnapshot, EdgeType, GraphSnapshot, NodeType,
};
pub use types::{
    AllocKind, AllocSample, AllocSamplerFn, CollectStats, FinalizerFn, FrameMark,
    GcEvent, GcHandleKind, GcKind, GcObserver, HeapSnapshot, HeapStats, ObjectStats,
    ObserverId, RootHandle, SnapshotCoverage, WeakRef,
};
