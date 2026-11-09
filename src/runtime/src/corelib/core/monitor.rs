//! `Monitor` —— z42 同步原语唯一的原生底座（store-sync-values-in-heap）。
//!
//! **不变量：原生层不持有任何 GC 值。** `Std.Threading.Mutex<T>` / `RwLock<T>` / `Channel<T>`
//! 用 z42 写在 Monitor 之上，它们的值 / 队列缓冲区是 z42 对象的普通字段——GC 按对象图追踪、
//! 按常规写屏障记录、随拥有者回收。Monitor 本身只记「谁持有 + 有几个线程在等」。
//!
//! 取代的旧实现（`sync.rs`）把 `Value` 存进 Rust 侧的 mpsc 队列 / parking_lot 锁，GC 根扫描器看不见
//! ⇒ 值只被原语持有时一次回收就被收掉。`sync.rs` 在阶段 1 作为种子例外保留（见其模块头）。
//!
//! # 语义
//!
//! - `enter`：阻塞直到持有；**同线程重入抛 `Std.Threading.LockRecursionException`**（不是死锁）。
//! - `try_enter`：不阻塞；被任何线程（含本线程）持有时返回 false。
//! - `exit`：释放；非持有者调用抛 `Std.Threading.SynchronizationLockException`（.NET `Monitor.Exit` 同款）。
//! - `wait`：原子地释放 → 等一次 `exit` 的信号 → 重新持有。**允许伪唤醒**，调用方必须
//!   `while (!条件) Wait()`。非持有者抛 `SynchronizationLockException`。
//!
//! builtin 是 ABI v2 的类型化函数（`typed` 模块）：失败从 Rust 抛上述类，句柄不是 Monitor →
//! `ArgumentException` / `ArgumentNullException`。
//!
//! # 两个条件变量
//!
//! `entry_cv` 挂「等进入」的线程，`signal_cv` 挂「在 `wait` 里等信号」的线程。`exit` 唤醒一个进入者
//! 并广播所有等信号者；`wait` 释放持有时**只唤醒进入者**。若共用一个条件变量，两个同时在 `wait` 的
//! 线程（如两个消费者等同一个空队列）会在各自释放持有时互相唤醒 → 醒来条件不满足再 `wait` → 再唤醒
//! 对方……无限空转烧 CPU。
//!
//! # 阻塞、safepoint 与内部锁的顺序（design D4）
//!
//! 1. 阻塞必须在 park 之内，否则另一线程发起 GC 时等不到本线程 → 死锁。结构上保证：阻塞的部分是
//!    builtin 返回的 [`Blocking::new`] 闭包（`Send + 'static`，只握 `Arc<Monitor>` 与 `ThreadId`），
//!    ABI 在 park 状态下运行它，碰不到堆、也就不可能分配。
//! 2. **离开 park 之前必须先释放内部 `state` 锁。** park 的退出可能要等 STW 结束；若此时还攥着
//!    `state`，另一个正在 `exit` 里抢 `state` 的线程（它既不在 safepoint 也没 park）就会让
//!    GC 等它、它等我、我等 GC。闭包返回时 `state` 的 guard 已 drop，park 在闭包返回之后才结束。
//! 3. 阻塞前先把 `Arc<Monitor>` 从句柄对象里克隆出来交给 park 闭包——阻塞期间 park 的线程
//!    不得再碰堆（collector 可能正在处理这个对象），手里只留原生的 `Arc`。不阻塞的路径只借用
//!    （[`monitor_ref`]），不克隆。
//!
//! 快路径（无争用）不 park：park 的进出要拿全局 phase 锁并 `notify_all`，不能每次加锁都付。
//! `state` 只在 O(1) 临界区里被持有，短暂等它不需要 park。

use std::sync::Arc;
use std::thread::ThreadId;
use std::time::Duration;

use crate::exec::call::builtin_abi::{Blocking, BuiltinError, IntoRet};
use crate::object::{NativeData, TypeDesc};
use crate::object::Value;
use crate::context::VmContext;

pub use crate::object::Monitor;

// ── builtins（ABI v2）────────────────────────────────────────────────────────

/// 从句柄对象里克隆出 `Arc<Monitor>`（D4 第 3 条：阻塞时只持有原生句柄）。`MonitorNative` 是公开
/// 类，用户代码能传进任何值：null → `ArgumentNullException`，别的对象 → `ArgumentException`。
#[cfg(test)]
pub(crate) fn monitor_arg(h: &Value) -> Result<Arc<Monitor>, BuiltinError> {
    monitor_ref(h).map(Arc::clone)
}

/// 句柄里的 `Arc<Monitor>`，借用而不克隆：不阻塞的路径（`exit` / `try_enter` / `enter` 的快路径）
/// 用它，省掉一对跨线程争用同一缓存行的引用计数原子操作；要阻塞时 [`Monitor::begin_enter`] /
/// [`Monitor::begin_wait`] 自己克隆一份交给 park 闭包（D4 第 3 条）。
#[inline]
pub(crate) fn monitor_ref(h: &Value) -> Result<&Arc<Monitor>, BuiltinError> {
    if let Value::Object(rc) = h {
        if let NativeData::Monitor(m) = rc.borrow().native() {
            return Ok(m);
        }
    }
    Err(not_a_monitor(h))
}

#[cold]
#[inline(never)]
fn not_a_monitor(h: &Value) -> BuiltinError {
    match h {
        Value::Null => BuiltinError::ArgumentNull("Value cannot be null. (Parameter 'monitor')".into()),
        _ => BuiltinError::Argument("The object is not a monitor handle (MonitorNative.New). (Parameter 'monitor')".into()),
    }
}

#[z42_macros::builtins]
pub mod typed {
    use super::{monitor_handle_type_desc, monitor_ref, Monitor};
    use crate::exec::call::builtin_abi::{Blocking, BuiltinError};
    use crate::object::NativeData;
    use crate::object::Value;
    use crate::context::VmContext;
    use std::sync::Arc;

    /// `MonitorNative.New() -> object` —— 不透明句柄，原生状态在 `NativeData::Monitor`。
    #[builtin(name = "__monitor_new")]
    pub fn monitor_new(ctx: &VmContext) -> Value {
        ctx.heap().alloc_object(monitor_handle_type_desc(), Vec::new(), NativeData::Monitor(Arc::new(Monitor::new())))
    }

    /// 争用时 park 等（`Monitor::begin_enter`）；同线程重入 → `LockRecursionException`。
    #[builtin(name = "__monitor_enter")]
    pub fn monitor_enter(ctx: &VmContext, monitor: &Value) -> Result<Blocking<()>, BuiltinError> {
        monitor_ref(monitor)?.begin_enter(ctx)
    }

    #[builtin(name = "__monitor_try_enter")]
    pub fn monitor_try_enter(monitor: &Value) -> Result<bool, BuiltinError> {
        Ok(monitor_ref(monitor)?.try_enter())
    }

    /// 非持有者 → `SynchronizationLockException`。
    #[builtin(name = "__monitor_exit")]
    pub fn monitor_exit(monitor: &Value) -> Result<(), BuiltinError> {
        monitor_ref(monitor)?.exit()
    }

    /// 非持有者 → `SynchronizationLockException`；其余整段 park。
    #[builtin(name = "__monitor_wait")]
    pub fn monitor_wait(monitor: &Value) -> Result<Blocking<()>, BuiltinError> {
        monitor_ref(monitor)?.begin_wait()
    }

    /// Timed wait, sharing ownership and GC parking with the infinite form.
    #[builtin(name = "__monitor_wait_for")]
    pub fn monitor_wait_for(monitor: &Value, timeout_ms: i64) -> Result<Blocking<bool>, BuiltinError> {
        monitor_ref(monitor)?.begin_wait_for(timeout_ms)
    }
}

/// 句柄的合成 TypeDesc（无字段；前例 `object.rs::weak_handle_type_desc`）。
fn monitor_handle_type_desc() -> Arc<TypeDesc> {
    use std::sync::OnceLock;
    static CACHE: OnceLock<Arc<TypeDesc>> = OnceLock::new();
    CACHE.get_or_init(|| Arc::new(TypeDesc {
        name: "Std.Threading.MonitorHandle".to_string(),
        base_name: None,
        class_flags: 0,
        visibility: 0,
        fields: Vec::new(),
        field_index: crate::zpkg::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::zpkg::NameIndex::new(),
        cold: None,
        id: crate::metadata::ids::tokens::TypeId::UNRESOLVED,
    })).clone()
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod monitor_tests;
