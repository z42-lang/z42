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
//! - `enter`：阻塞直到持有；**同线程重入报错**（不是死锁）。
//! - `try_enter`：不阻塞；被任何线程（含本线程）持有时返回 false。
//! - `exit`：释放；非持有者调用报错。
//! - `wait`：原子地释放 → 等一次 `exit` 的信号 → 重新持有。**允许伪唤醒**，调用方必须
//!   `while (!条件) Wait()`。
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
//! 1. 阻塞必须在 [`NativeParkGuard`] 之内，否则另一线程发起 GC 时等不到本线程 → 死锁。
//! 2. **离开 park 之前必须先释放内部 `state` 锁。** park 的退出可能要等 STW 结束；若此时还攥着
//!    `state`，另一个正在 `exit` 里抢 `state` 的线程（它既不在 safepoint 也没 park）就会让
//!    GC 等它、它等我、我等 GC。代码里靠**声明顺序**保证：`_park` 先声明（后 drop），`st` 后声明（先 drop）。
//! 3. 阻塞前先把 `Arc<Monitor>` 从句柄对象里克隆出来、释放对象借用（[`monitor_arg`]）——
//!    带着对象借用阻塞会挡住 GC 对该对象的处理。
//!
//! 快路径（无争用）不 park：park 的进出要拿全局 phase 锁并 `notify_all`，不能每次加锁都付。
//! `state` 只在 O(1) 临界区里被持有，短暂等它不需要 park。

use std::sync::Arc;
use std::thread::ThreadId;

use anyhow::{bail, Result};
use parking_lot::{Condvar, Mutex};

use crate::gc::NativeParkGuard;
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

    pub fn enter(&self, ctx: &VmContext) -> Result<()> {
        let me = me();
        {
            let mut st = self.state.lock();
            if st.owner == Some(me) {
                bail!("Monitor is not reentrant: the calling thread already holds it");
            }
            if st.owner.is_none() {
                st.owner = Some(me);
                return Ok(());
            }
        }
        super::sync_contention::monitor_contended(ctx, || {
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
                    return;
                }
            }
            let _park = NativeParkGuard::enter(ctx);
            let mut st = self.state.lock();
            st.entry_waiters += 1;
            while st.owner.is_some() {
                self.entry_cv.wait(&mut st);
            }
            st.entry_waiters -= 1;
            st.owner = Some(me);
        });
        Ok(())
    }

    pub fn exit(&self) -> Result<()> {
        let mut st = self.state.lock();
        if st.owner != Some(me()) {
            bail!("Monitor exit: the calling thread does not hold the monitor");
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

    pub fn wait(&self, ctx: &VmContext) -> Result<()> {
        let me = me();
        let _park = NativeParkGuard::enter(ctx);
        let mut st = self.state.lock();
        if st.owner != Some(me) {
            bail!("Monitor wait: the calling thread does not hold the monitor");
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

// ── builtins ────────────────────────────────────────────────────────────────

/// `__monitor_new() -> object` —— 不透明句柄，原生状态在 `NativeData::Monitor`。
pub fn builtin_monitor_new(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    Ok(ctx.heap().alloc_object(
        monitor_handle_type_desc(),
        Vec::new(),
        NativeData::Monitor(Arc::new(Monitor::new())),
    ))
}

/// `__monitor_enter(h) -> Null`
pub fn builtin_monitor_enter(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    monitor_arg(args, "__monitor_enter")?.enter(ctx)?;
    Ok(Value::Null)
}

/// `__monitor_try_enter(h) -> bool`
pub fn builtin_monitor_try_enter(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(monitor_arg(args, "__monitor_try_enter")?.try_enter()))
}

/// `__monitor_exit(h) -> Null`
pub fn builtin_monitor_exit(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    monitor_arg(args, "__monitor_exit")?.exit()?;
    Ok(Value::Null)
}

/// `__monitor_wait(h) -> Null`
pub fn builtin_monitor_wait(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    monitor_arg(args, "__monitor_wait")?.wait(ctx)?;
    Ok(Value::Null)
}

/// 从句柄对象里克隆出 `Arc<Monitor>`，**并在返回前释放对象借用**（D4 第 3 条）。
fn monitor_arg(args: &[Value], name: &str) -> Result<Arc<Monitor>> {
    match args.first() {
        Some(Value::Object(rc)) => {
            let obj = rc.borrow();
            match obj.native() {
                NativeData::Monitor(m) => Ok(Arc::clone(m)),
                _ => bail!("{name}: expected a monitor handle"),
            }
        }
        Some(other) => bail!("{name}: expected a monitor handle, got {:?}", other),
        None => bail!("{name}: missing monitor handle"),
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
