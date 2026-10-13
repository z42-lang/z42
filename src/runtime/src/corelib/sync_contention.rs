//! User-lock contention probe (add-concurrency-probes, script-profiling P1b).
//!
//! Every z42 user lock (`Std.Threading.Mutex` / `RwLock` / `Channel`) is built on the VM
//! `Monitor` (`corelib/monitor.rs`); its contended enter path is timed here. Under the
//! `profile-contention` cargo feature a contended acquire bumps `VmCore.lock_contentions`
//! and adds its wait to `VmCore.lock_wait_us`; without the feature the probe is a
//! zero-sized type and both calls inline away (the default build's acquire path is
//! unchanged).

use crate::vm_context::VmContext;

/// `Monitor::begin_enter` 的慢路径（已确认争用）计时：`profile-contention` 下计一次争用并计时
/// 「自旋 + park 等待」整段（`begin` 在自旋前、`end` 在拿到锁后——可能在 park 闭包里，所以
/// 只握 `Arc<VmCore>`、`Send + 'static`）；默认构建是零大小类型，两个调用都内联成空。
pub(crate) struct MonitorContention {
    #[cfg(feature = "profile-contention")]
    core: std::sync::Arc<crate::vm_context::VmCore>,
    #[cfg(feature = "profile-contention")]
    t0: std::time::Instant,
}

impl MonitorContention {
    #[cfg(feature = "profile-contention")]
    pub(crate) fn begin(ctx: &VmContext) -> Self {
        ctx.core.lock_contentions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        MonitorContention { core: std::sync::Arc::clone(&ctx.core), t0: std::time::Instant::now() }
    }

    #[cfg(not(feature = "profile-contention"))]
    #[inline(always)]
    pub(crate) fn begin(_ctx: &VmContext) -> Self {
        MonitorContention {}
    }

    #[cfg(feature = "profile-contention")]
    pub(crate) fn end(&self) {
        self.core.lock_wait_us.fetch_add(self.t0.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(not(feature = "profile-contention"))]
    #[inline(always)]
    pub(crate) fn end(&self) {}
}
