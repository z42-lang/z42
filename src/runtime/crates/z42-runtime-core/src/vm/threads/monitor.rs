//! Monitor state transitions and parked blocking operations.
use std::{sync::Arc, thread::ThreadId, time::Duration};
use crate::object::Monitor;
use crate::vm::{context::VmContext, exec::call::builtin_abi::{Blocking, BuiltinError, IntoRet}};
/// `enter` 慢路径 park 之前的自旋轮数：前 `SPIN_BUSY_ROUNDS` 轮忙等，其余轮 `yield_now`。
const SPIN_ROUNDS: u32 = 40;
const SPIN_BUSY_ROUNDS: u32 = 32;

fn me() -> ThreadId {
    std::thread::current().id()
}

/// 非持有者 `Exit` / `Wait`：.NET `Monitor.Exit` / `Wait` 的异常与原文。
#[cold]
#[inline(never)]
fn not_held() -> BuiltinError {
    BuiltinError::Exception {
        class: "Std.Threading.SynchronizationLockException",
        message: "Object synchronization method was called from an unsynchronized block of code.".into(),
    }
}

/// 同线程再次 `Enter`：.NET 不可重入锁（`ReaderWriterLockSlim` 的 NoRecursion 策略）的异常。
#[cold]
#[inline(never)]
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
        let probe = crate::vm::diagnostics::contention::MonitorContention::begin(ctx);
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
        Ok(Blocking::new(move || m.release_and_wait(me, None).map(|_| ())))
    }

    /// Timed signal wait; reacquiring ownership remains unbounded.
    pub fn begin_wait_for(self: &Arc<Self>, timeout_ms: i64) -> Result<Blocking<bool>, BuiltinError> {
        if timeout_ms < 0 {
            return Err(BuiltinError::ArgumentOutOfRange("timeoutMs must be non-negative".into()));
        }
        #[cfg(target_arch = "wasm32")]
        { Err(BuiltinError::NotSupported("timed monitor waits require thread support".into())) }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let me = me();
            let m = Arc::clone(self);
            let timeout = Duration::from_millis(timeout_ms as u64);
            Ok(Blocking::new(move || m.release_and_wait(me, Some(timeout))))
        }
    }

    pub fn wait(self: &Arc<Self>, ctx: &VmContext) -> Result<(), BuiltinError> {
        self.begin_wait()?.into_ret(ctx).map(|_| ()).map_err(|e| *e)
    }

    /// park 状态下运行：原子地释放 → 等一次 `exit` 的信号 → 重新持有。
    fn release_and_wait(&self, me: ThreadId, timeout: Option<Duration>) -> Result<bool, BuiltinError> {
        let mut st = self.state.lock();
        if st.owner != Some(me) {
            return Err(not_held());
        }
        st.owner = None;
        if st.entry_waiters > 0 {
            self.entry_cv.notify_one();
        }
        st.signal_waiters += 1;
        let signalled = match timeout {
            Some(duration) => {
                let result = self.signal_cv.wait_for(&mut st, duration);
                !duration.is_zero() && !result.timed_out()
            }
            None => { self.signal_cv.wait(&mut st); true }
        };
        st.signal_waiters -= 1;
        st.entry_waiters += 1;
        while st.owner.is_some() {
            self.entry_cv.wait(&mut st);
        }
        st.entry_waiters -= 1;
        st.owner = Some(me);
        Ok(signalled)
    }
}
