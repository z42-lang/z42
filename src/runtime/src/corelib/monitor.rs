//! `Monitor` —— z42 同步原语唯一的原生底座（store-sync-values-in-heap, 2026-09-14）。
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
//! 3. 阻塞前先把 `Arc<Monitor>` 从句柄对象里克隆出来（[`monitor_arg`]）——阻塞期间 park 的线程
//!    不得再碰堆（collector 可能正在处理这个对象），手里只留原生的 `Arc`。
//!
//! 快路径（无争用）不 park：park 的进出要拿全局 phase 锁并 `notify_all`，不能每次加锁都付。
//! `state` 只在 O(1) 临界区里被持有，短暂等它不需要 park。

use std::sync::Arc;
use std::thread::ThreadId;

use parking_lot::{Condvar, Mutex};

use crate::corelib::abi::{Blocking, BuiltinError, IntoRet};
use crate::metadata::types::{NativeData, TypeDesc};
use crate::metadata::Value;
use crate::vm_context::VmContext;

#[derive(Debug, Default)]
pub struct Monitor {
    state:     Mutex<MonitorState>,
    entry_cv:  Condvar,
    signal_cv: Condvar,
}

#[derive(Debug, Default)]
struct MonitorState {
    owner:          Option<ThreadId>,
    entry_waiters:  u32,
    signal_waiters: u32,
}

/// `enter` 慢路径 park 之前的自旋轮数：前 `SPIN_BUSY_ROUNDS` 轮忙等，其余轮 `yield_now`。
const SPIN_ROUNDS: u32 = 40;
const SPIN_BUSY_ROUNDS: u32 = 32;

fn me() -> ThreadId {
    std::thread::current().id()
}

/// 非持有者 `Exit` / `Wait`：.NET `Monitor.Exit` / `Wait` 的异常与原文。
fn not_held() -> BuiltinError {
    BuiltinError::Exception {
        class: "Std.Threading.SynchronizationLockException",
        message: "Object synchronization method was called from an unsynchronized block of code.".into(),
    }
}

/// 同线程再次 `Enter`：.NET 不可重入锁（`ReaderWriterLockSlim` 的 NoRecursion 策略）的异常。
fn recursion() -> BuiltinError {
    BuiltinError::Exception {
        class: "Std.Threading.LockRecursionException",
        message: "Recursive lock acquisition is not allowed: the calling thread already holds this lock.".into(),
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_enter(&self) -> bool {
        let mut st = self.state.lock();
        if st.owner.is_some() {
            return false;
        }
        st.owner = Some(me());
        true
    }

    /// `enter` 的不阻塞部分：重入检查、无争用快路径、短暂自旋，都在 unpark 状态下做；
    /// 仍拿不到时返回 park 运行的等待（[`Blocking::new`]，闭包只握 `Arc<Monitor>`）。
    pub fn begin_enter(self: &Arc<Self>, ctx: &VmContext) -> Result<Blocking<()>, BuiltinError> {
        let me = me();
        {
            let mut st = self.state.lock();
            if st.owner == Some(me) {
                return Err(recursion());
            }
            if st.owner.is_none() {
                st.owner = Some(me);
                return Ok(Blocking::ready(()));
            }
        }
        let probe = super::sync_contention::MonitorContention::begin(ctx);
        // 先短暂自旋再 park：Monitor 保护的都是 O(1) 的 z42 片段，持有者通常转眼就释放；
        // 而 park 一次要拿全局 phase 锁 + notify_all、被唤醒还要走条件变量。实测跨线程
        // 有界 Channel（生产者 / 消费者高频争用）不自旋时比旧 mpsc 实现慢 2.15 倍。
        // 自旋上限很小（几十次 spin_loop + 几次 yield），持有者真在长 body 里时很快就转去 park。
        for i in 0..SPIN_ROUNDS {
            if i < SPIN_BUSY_ROUNDS {
                std::hint::spin_loop();
            } else {
                std::thread::yield_now();
            }
            let mut st = self.state.lock();
            if st.owner.is_none() {
                st.owner = Some(me);
                probe.end();
                return Ok(Blocking::ready(()));
            }
        }
        let m = Arc::clone(self);
        Ok(Blocking::new(move || {
            m.wait_for_entry(me);
            probe.end();
            Ok(())
        }))
    }

    /// 阻塞直到持有（同线程重入 → `LockRecursionException`）。Rust 侧直接用（测试）；
    /// builtin 走 [`Monitor::begin_enter`]。
    pub fn enter(self: &Arc<Self>, ctx: &VmContext) -> Result<(), BuiltinError> {
        self.begin_enter(ctx)?.into_ret(ctx).map(|_| ()).map_err(|e| *e)
    }

    /// park 状态下运行：排队等 `owner` 变空，再持有。
    fn wait_for_entry(&self, me: ThreadId) {
        let mut st = self.state.lock();
        st.entry_waiters += 1;
        while st.owner.is_some() {
            self.entry_cv.wait(&mut st);
        }
        st.entry_waiters -= 1;
        st.owner = Some(me);
    }

    pub fn exit(&self) -> Result<(), BuiltinError> {
        let mut st = self.state.lock();
        if st.owner != Some(me()) {
            return Err(not_held());
        }
        st.owner = None;
        if st.entry_waiters > 0 {
            self.entry_cv.notify_one();
        }
        if st.signal_waiters > 0 {
            self.signal_cv.notify_all();
        }
        Ok(())
    }

    /// `wait` 整段 park 运行：持有检查、释放 → 等信号 → 重新持有，一次拿内部锁（与先在
    /// unpark 状态下单独查一次持有相比，争用的 Channel 少一次内部锁往返）。非持有者的错误
    /// 是纯 Rust 数据，在 park 中构造无妨。
    pub fn begin_wait(self: &Arc<Self>) -> Result<Blocking<()>, BuiltinError> {
        let me = me();
        let m = Arc::clone(self);
        Ok(Blocking::new(move || m.release_and_wait(me)))
    }

    pub fn wait(self: &Arc<Self>, ctx: &VmContext) -> Result<(), BuiltinError> {
        self.begin_wait()?.into_ret(ctx).map(|_| ()).map_err(|e| *e)
    }

    /// park 状态下运行：原子地释放 → 等一次 `exit` 的信号 → 重新持有。
    fn release_and_wait(&self, me: ThreadId) -> Result<(), BuiltinError> {
        let mut st = self.state.lock();
        if st.owner != Some(me) {
            return Err(not_held());
        }
        st.owner = None;
        if st.entry_waiters > 0 {
            self.entry_cv.notify_one();
        }
        st.signal_waiters += 1;
        self.signal_cv.wait(&mut st);
        st.signal_waiters -= 1;
        st.entry_waiters += 1;
        while st.owner.is_some() {
            self.entry_cv.wait(&mut st);
        }
        st.entry_waiters -= 1;
        st.owner = Some(me);
        Ok(())
    }
}

// ── builtins（ABI v2）────────────────────────────────────────────────────────

/// 从句柄对象里克隆出 `Arc<Monitor>`（D4 第 3 条：阻塞时只持有原生句柄）。`MonitorNative` 是公开
/// 类，用户代码能传进任何值：null → `ArgumentNullException`，别的对象 → `ArgumentException`。
pub(crate) fn monitor_arg(h: &Value) -> Result<Arc<Monitor>, BuiltinError> {
    match h {
        Value::Object(rc) => match rc.borrow().native() {
            NativeData::Monitor(m) => Ok(Arc::clone(m)),
            _ => Err(BuiltinError::Argument("The object is not a monitor handle (MonitorNative.New). (Parameter 'monitor')".into())),
        },
        Value::Null => Err(BuiltinError::ArgumentNull("Value cannot be null. (Parameter 'monitor')".into())),
        _ => Err(BuiltinError::Argument("The object is not a monitor handle (MonitorNative.New). (Parameter 'monitor')".into())),
    }
}

#[z42_macros::builtins]
pub mod typed {
    use super::{monitor_arg, monitor_handle_type_desc, Monitor};
    use crate::corelib::abi::{Blocking, BuiltinError};
    use crate::metadata::types::NativeData;
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use std::sync::Arc;

    /// `MonitorNative.New() -> object` —— 不透明句柄，原生状态在 `NativeData::Monitor`。
    #[builtin(name = "__monitor_new")]
    pub fn monitor_new(ctx: &VmContext) -> Value {
        ctx.heap().alloc_object(monitor_handle_type_desc(), Vec::new(), NativeData::Monitor(Arc::new(Monitor::new())))
    }

    /// 争用时 park 等（`Monitor::begin_enter`）；同线程重入 → `LockRecursionException`。
    #[builtin(name = "__monitor_enter")]
    pub fn monitor_enter(ctx: &VmContext, monitor: &Value) -> Result<Blocking<()>, BuiltinError> {
        monitor_arg(monitor)?.begin_enter(ctx)
    }

    #[builtin(name = "__monitor_try_enter")]
    pub fn monitor_try_enter(monitor: &Value) -> Result<bool, BuiltinError> {
        Ok(monitor_arg(monitor)?.try_enter())
    }

    /// 非持有者 → `SynchronizationLockException`。
    #[builtin(name = "__monitor_exit")]
    pub fn monitor_exit(monitor: &Value) -> Result<(), BuiltinError> {
        monitor_arg(monitor)?.exit()
    }

    /// 非持有者 → `SynchronizationLockException`；其余整段 park。
    #[builtin(name = "__monitor_wait")]
    pub fn monitor_wait(monitor: &Value) -> Result<Blocking<()>, BuiltinError> {
        monitor_arg(monitor)?.begin_wait()
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
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })).clone()
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
mod monitor_tests;
