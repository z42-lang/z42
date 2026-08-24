//! Safepoint sampling profiler —— z42 调用栈采样火焰图 + perfetto 采样 trace。
//!
//! 脚本性能分析程序 P2（最后一个单元）。现有 `xtask profile --cpu` 的 samply 只给 **native**
//! 栈（JIT machine frame / interp dispatch loop），看不出「哪个 **z42 函数**热」。本模块按 z42
//! 源函数聚合采样，产两路输出（源自**同一次** safepoint 栈快照）：
//!
//! - **folded → 火焰图**：`Main;foo;bar <count>` 每行（inferno / flamegraph.pl 标准输入，聚合、无时间轴）。
//! - **(ts_us, 栈) 样本序列 → perfetto/chrome trace**（采样型时间线，仅 `Z42_TRACE_OUT` 设时记录）。
//!
//! # 机制（DRAFT D1/D2/D3/D5）
//!
//! 复用**协作式 safepoint 轮询**，零信号 / 零 ptrace：一个后台定时线程按 `Z42_SAMPLE_HZ` 频率
//! 置 [`Sampler::sample_pending`] flag；mutator 在**已经要跑的** `check_safepoint_slow`（Idle 末，
//! 已被 throttle 到 ~1/1024）见 flag → 快照当前 `ctx.call_stack` 的 `VmFrame.func_name` → 累加。
//! 默认关（`Z42_SAMPLE_HZ` 未设）时：无后台线程、flag 永不置、热路径只多一次 atomic load → **零成本**
//! （故用运行时 flag gate，不需 cargo feature；区别于 P1b contention 探针在每次 lock acquire、需编译期 gate）。
//!
//! # perfetto trace = 采样型（非 span 埋点）
//!
//! chrome legacy JSON trace 原生支持**采样** profiling（`ph:"P"` sample 事件 + `stackFrames` 帧树）。
//! 故 perfetto 输出**不依赖** span 埋点基建（每帧 enter/exit 计时，那才违反默认零成本）——用同一次
//! 采样的栈快照 intern 进帧树、记 `(ts_us, leaf_id)`，退出序列化。偏差同 D1（只在 safepoint 采），
//! perfetto 时间线继承之，不额外引入开销。

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use crate::vm_context::VmContext;

/// 每个采样 = 后台线程置 flag、mutator 在下一个 safepoint 快照一次栈。
///
/// `enabled=false`（默认）时：无后台线程、`data` 空、`maybe_sample` 永不被调用
/// （调用点 gate 在 [`Sampler::enabled`] 一次 load）。
pub struct Sampler {
    /// `Z42_SAMPLE_HZ` 设即 `true`。
    enabled: bool,
    /// `Z42_TRACE_OUT` 设即 `true` —— 额外记录 per-sample 时间线（perfetto）。未设时只累加 folded（省内存）。
    trace_enabled: bool,
    /// 后台线程置 `true`，mutator 在 safepoint `swap(false)`。
    sample_pending: Arc<AtomicBool>,
    /// Drop 时置 `true`，让定时线程退出其 sleep 循环。
    stop: Arc<AtomicBool>,
    /// 采样时间戳基准 t0（`Z42_TRACE_OUT` 才有意义）。
    start: Instant,
    data: Mutex<SamplerData>,
    /// 定时线程句柄（detached-ish：Drop 置 `stop` 后不 join，线程见 flag 自退）。
    _thread: Option<std::thread::JoinHandle<()>>,
}

/// 采样累加状态。`folded` 恒填充；帧树 / `samples` 仅 `trace_enabled` 时填充。
#[derive(Default)]
struct SamplerData {
    /// folded stack（`;` join、栈底在左）→ 采样计数。喂火焰图。
    folded: HashMap<String, u64>,
    // ── 以下仅 trace_enabled 时填充（perfetto 采样时间线）──────────────────────
    /// intern 的帧树；index = 帧 id。
    frames: Vec<FrameNode>,
    /// `(parent_id | u32::MAX, name)` → 帧 index，去重帧树节点。
    frame_ids: HashMap<(u32, Arc<str>), u32>,
    /// `(ts_us, leaf frame id)` 采样序列，按时间顺序。
    samples: Vec<(u64, u32)>,
}

/// perfetto `stackFrames` 树的一个节点。`parent == u32::MAX` 为根。
struct FrameNode {
    name: Arc<str>,
    parent: u32,
}

const NO_PARENT: u32 = u32::MAX;

/// 安全上限：trace 采样序列最多这么多条（12 B/条 → ~120 MB）。超出停止记录 trace 样本
/// （folded 仍继续），一次性 stderr 警告。防病态长跑 OOM。
const MAX_TRACE_SAMPLES: usize = 10_000_000;

impl Sampler {
    /// 启动采样：spawn 后台定时线程（`hz` 次/秒置 flag）。`hz` 会被夹到 `>= 1`。
    /// `trace_enabled` 决定是否额外记 perfetto 时间线。
    pub fn start(hz: u32, trace_enabled: bool) -> Self {
        let hz = hz.max(1);
        let sample_pending = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let interval = std::time::Duration::from_micros(1_000_000 / hz as u64);
        let thread = {
            let pending = Arc::clone(&sample_pending);
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("z42-sampler".into())
                .spawn(move || {
                    // 只置 flag、不碰累加器 —— mutator 在 safepoint 才快照栈。
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(interval);
                        pending.store(true, Ordering::Relaxed);
                    }
                })
                .ok()
        };
        Self {
            enabled: true,
            trace_enabled,
            sample_pending,
            stop,
            start: Instant::now(),
            data: Mutex::new(SamplerData::default()),
            _thread: thread,
        }
    }

    /// 测试专用：enabled 但**不** spawn 定时线程（测试自己 `force_pending` 或直接调 `record`）。
    #[cfg(test)]
    pub(crate) fn for_test(trace_enabled: bool) -> Self {
        Self {
            enabled: true,
            trace_enabled,
            sample_pending: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            start: Instant::now(),
            data: Mutex::new(SamplerData::default()),
            _thread: None,
        }
    }

    /// 采样关（默认）：无后台线程、`enabled=false`。
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            trace_enabled: false,
            sample_pending: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
            start: Instant::now(),
            data: Mutex::new(SamplerData::default()),
            _thread: None,
        }
    }

    /// 采样是否开启。调用点（safepoint hook）先 gate 在这个一次 load 上。
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// safepoint 命中点：若定时线程置了 flag，则 `swap(false)` 并快照当前 z42 调用栈。
    ///
    /// 前置：调用点已确认 `enabled()`；且此处不持任何 GC 锁（`check_safepoint_slow` 的 Idle
    /// 末，gc_phase 锁已释放）。锁序：只取 `ctx.call_stack` 再取 `self.data`，两者与 gc_phase 无嵌套。
    pub fn maybe_sample(&self, ctx: &VmContext) {
        if !self.sample_pending.swap(false, Ordering::Relaxed) {
            return;
        }
        // 快照栈：栈底在左（call_stack 顺序，index 0 = 最外层 Main）。只读 func_name。
        let names: Vec<Arc<str>> = {
            let cs = ctx.call_stack.lock();
            cs.iter().map(|f| f.func_name.clone()).collect()
        };
        if names.is_empty() {
            return; // 空栈不产坏行
        }
        let ts_us = self.start.elapsed().as_micros() as u64;
        self.record(&names, ts_us);
    }

    /// 累加一次采样（testable 核心，与 [`Self::maybe_sample`] 的栈快照解耦）。
    /// folded 计数**恒**累加；`trace_enabled` 时再 intern 帧树 + push `(ts_us, leaf_id)`。
    fn record(&self, names: &[Arc<str>], ts_us: u64) {
        if names.is_empty() {
            return;
        }
        let mut d = self.data.lock();
        // ① folded 计数（火焰图）。
        let folded = names.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(";");
        *d.folded.entry(folded).or_insert(0) += 1;
        // ② perfetto 采样时间线（仅 trace_enabled）。
        if self.trace_enabled && d.samples.len() < MAX_TRACE_SAMPLES {
            let leaf = intern_stack(&mut d, names);
            d.samples.push((ts_us, leaf));
        } else if self.trace_enabled && d.samples.len() == MAX_TRACE_SAMPLES {
            eprintln!(
                "z42: sampler trace samples hit cap ({MAX_TRACE_SAMPLES}); \
                 timeline truncated (folded flamegraph still complete)"
            );
            // 再 push 一条 sentinel 避免重复打印（len 变 MAX+1，上面分支不再进）。
            d.samples.push((ts_us, NO_PARENT));
        }
    }

    /// 写 folded stacks 到 `path`，按 count 降序（稳定：同 count 按 folded 字典序）。
    /// 无采样命中 → 写空文件（不产坏行）。
    pub fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        let d = self.data.lock();
        let mut rows: Vec<(&String, &u64)> = d.folded.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        for (folded, count) in rows {
            writeln!(f, "{folded} {count}")?;
        }
        f.flush()
    }

    /// 写 chrome legacy JSON trace（perfetto 直接 import）到 `path`。
    /// `traceEvents`（`ph:"P"` sample 事件）+ `stackFrames`（帧树）。
    /// 仅 `trace_enabled` 时有数据；否则写一个空 trace（合法 JSON）。
    pub fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        let d = self.data.lock();
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        write!(f, "{{\"traceEvents\":[")?;
        // 采样事件：sentinel（leaf==NO_PARENT，cap 溢出标记）跳过。
        let mut first = true;
        for &(ts, leaf) in d.samples.iter() {
            if leaf == NO_PARENT {
                continue;
            }
            let name = d.frames.get(leaf as usize).map(|n| n.name.as_ref()).unwrap_or("?");
            if !first {
                write!(f, ",")?;
            }
            first = false;
            write!(
                f,
                "{{\"ph\":\"P\",\"name\":\"{}\",\"pid\":1,\"tid\":1,\"ts\":{},\"sf\":\"{}\"}}",
                json_escape(name),
                ts,
                leaf
            )?;
        }
        write!(f, "],\"stackFrames\":{{")?;
        for (id, node) in d.frames.iter().enumerate() {
            if id != 0 {
                write!(f, ",")?;
            }
            write!(f, "\"{}\":{{\"name\":\"{}\"", id, json_escape(&node.name))?;
            if node.parent != NO_PARENT {
                write!(f, ",\"parent\":\"{}\"", node.parent)?;
            }
            write!(f, "}}")?;
        }
        write!(f, "}}}}")?;
        f.flush()
    }

    /// 采样命中总数（测试 / 诊断用）。
    #[cfg(test)]
    pub fn sample_count(&self) -> u64 {
        self.data.lock().folded.values().sum()
    }

    /// 测试专用：强制置 pending flag（模拟后台线程）。
    #[cfg(test)]
    pub fn force_pending(&self) {
        self.sample_pending.store(true, Ordering::Relaxed);
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        // 让定时线程退出其 sleep 循环（下一次 wake 见 stop=true 即返回）。
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl std::fmt::Debug for Sampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sampler")
            .field("enabled", &self.enabled)
            .field("trace_enabled", &self.trace_enabled)
            .finish_non_exhaustive()
    }
}

/// 把一条自栈底到栈顶的帧名序列 intern 进帧树，返回**叶帧** id。
/// 逐层用 `(parent, name)` 去重复用节点。
fn intern_stack(d: &mut SamplerData, names: &[Arc<str>]) -> u32 {
    let mut parent = NO_PARENT;
    for name in names {
        let key = (parent, Arc::clone(name));
        parent = match d.frame_ids.get(&key) {
            Some(&id) => id,
            None => {
                let id = d.frames.len() as u32;
                d.frames.push(FrameNode { name: Arc::clone(name), parent });
                d.frame_ids.insert(key, id);
                id
            }
        };
    }
    parent
}

/// 最小 JSON 字符串转义（`"` / `\` / 控制字符）。z42 函数名基本是标识符 + `.`/`<`/`>`/`$`/`,`
/// （泛型 mangle），无一需转义，但对含特殊字符的名字仍安全。
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
#[path = "sampler_tests.rs"]
mod sampler_tests;
