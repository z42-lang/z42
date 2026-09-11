//! `Z42_GC_PHASES` —— 把一次停顿拆成各阶段的耗时（add-gc-phase-timing, 2026-09-11）。
//!
//! `Z42_GC_TRACE` 说得出「这次 Full 停了 77 ms」，说不出这 77 ms 花在哪。整条 GC
//! 停顿线（#565 / #566 / #569 / #570）的每一次定位都靠同一件事：**给每个阶段套一个
//! env 门控的计时器，看它的形状**——「25.6 ms 的 full mark 标了 95 万个对象」是正常的，
//! 「12 ms 的 mark 只标了 84 个对象」是缺陷。这个补丁此前每次都是手打、量完删掉，
//! 于是下一次又得重打一遍；这里把它固定下来。
//!
//! ```text
//! z42-gc:   reset marks                  8.21 ms
//! z42-gc:   full mark                   25.63 ms  (953988)
//! z42-gc:   sweep/scan objects           4.902 ms  (128301)
//! ...
//! z42-gc: Full  used 412.3M -> 118.7M  freed 293.6M  pause 41.2ms  (cycle 7)
//! ```
//!
//! 关掉时（默认）一个 [`PhaseTimer`] 就是一个 `None` + 一个 `Cell::set`，不取时钟、
//! 不分配、不输出；阶段本身是「每次回收几次」的量级，不在任何热路径上。

use std::cell::Cell;
use std::time::Instant;

/// 旋钮是否打开。`runtime_config()` 是一个 `OnceLock` 读，按阶段（每次回收十几次）
/// 调用足够便宜，不需要再缓存一层。
#[inline]
pub fn phases_enabled() -> bool {
    crate::config::runtime_config().gc_phases
}

/// 一个阶段的 RAII 计时器：构造时取时钟，析构时打一行。
///
/// 关掉时 `start` 是 `None`，`Drop` 直接跳过——不取时钟也不输出。
pub struct PhaseTimer {
    name: &'static str,
    start: Option<Instant>,
    /// 可选的「这一阶段处理了多少个」。形状判断几乎全靠它与耗时的比值
    /// （见模块文档里 mark 的两个例子），所以它和耗时同等重要。
    count: Cell<Option<u64>>,
}

impl PhaseTimer {
    /// 开一个阶段。`name` 用 `区域/阶段` 的形式（`sweep/scan objects`），
    /// 这样一次回收的输出按前缀就能折叠着看。
    #[inline]
    pub fn start(name: &'static str) -> Self {
        PhaseTimer {
            name,
            start: if phases_enabled() { Some(Instant::now()) } else { None },
            count: Cell::new(None),
        }
    }

    /// 记下这一阶段处理的条目数，随耗时一起打出来。关掉时是 no-op。
    #[inline]
    pub fn count(&self, n: usize) {
        if self.start.is_some() {
            self.count.set(Some(n as u64));
        }
    }
}

impl Drop for PhaseTimer {
    fn drop(&mut self) {
        if let Some(t0) = self.start {
            eprintln!("{}", format_phase(self.name, t0.elapsed(), self.count.get()));
        }
    }
}

/// 打一行与阶段行同前缀的自由文本。给「这次回收是被什么闸门触发的」这类**不是一段耗时、
/// 但同样属于这次停顿的解释**用——阶段行说的是「花在哪」，它说的是「为什么是现在」。
/// 关掉时 no-op。
#[inline]
pub fn note(args: std::fmt::Arguments<'_>) {
    if phases_enabled() {
        eprintln!("z42-gc:   {args}");
    }
}

/// 单独拆出来是为了能测——`Drop` 里的 `eprintln!` 测不到。
fn format_phase(name: &str, elapsed: std::time::Duration, count: Option<u64>) -> String {
    let ms = elapsed.as_secs_f64() * 1000.0;
    match count {
        Some(n) => format!("z42-gc:   {name:<26} {ms:>9.3} ms  ({n})"),
        None => format!("z42-gc:   {name:<26} {ms:>9.3} ms"),
    }
}

#[cfg(test)]
#[path = "phase_timer_tests.rs"]
mod phase_timer_tests;
