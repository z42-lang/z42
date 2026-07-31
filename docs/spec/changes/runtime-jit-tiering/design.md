# Design: 运行时 JIT/interp 分层执行 + IR 回收

> 对齐：2026-07-30。准则见 book [optimization-pipeline](../../../book/src/runtime/optimization-pipeline.md) 准则 2
> （控制内存/时间开销 · 分层升级旧层可回收 · 回收池化不抖动 OS）。

## Architecture

```
jit::run（入口）→ jit_call 分发每个 callee：
   resolve id → 三态 FnEntry 槽 + call_counts[id]（side table）
     ├ Compiled (ptr≠null)      → 原生调用（lock-free OnceLock::get）
     ├ Rejected (ptr==null)     → interp 执行（cross_zpkg_via_interp，负缓存不重扫）
     └ Unknown  (OnceLock 空)   → count++；
            count < 阈值         → interp 执行（cold tier）
            count ≥ 阈值 && 可编 → compile_fn → 填 Compiled 槽
            count ≥ 阈值 && 不可编 → 填 Rejected 槽（null ptr）
   [Phase 1.5] interp 的 Call/VCall 也查同一槽：Compiled → 原生（interp 感知 JIT）
   [Phase 2]   Compiled 且 Phase 1.5 保证不再被 interp 执行 → 回收 blocks（池化）
```

## Decisions

### Decision 1：调用计数放 side table（零 per-call 堆分配）
`Function` 在 `Arc<Module>` 后（Sync），不加可变字段。用 `JitModuleCtx.call_counts: Vec<AtomicU32>`（按 func id，
setup 时预分配 `module.functions.len()`，与 `fn_entries_by_id` 平行）。lock-free `fetch_add`。**预分配 → 零 per-call
分配**（准则 2 第 4 条）。lazy/by-name 目标用 `LazyTable` 平行槽的计数。

### Decision 2：三态用 null-ptr FnEntry（保 lock-free）
不改 `OnceLock<FnEntry>` 结构（保稳态无锁读）。第三态编码进 `FnEntry.ptr`：`ptr==null` = Rejected。
`resolve` 稳态:`OnceLock::get()` → `ptr==null` ? interp : 原生。Rejected 一次判定后缓存,**消除每-call 重扫
`jit_unsupported_reason`**。

### Decision 3：cold tier 复用 `cross_zpkg_via_interp`（实施期收窄为 jit_call-only）
未到阈值 / Rejected 的函数走现有 interp 兜底（`cross_zpkg_via_interp` → `interp::exec_function`）。
**实施期实证(重要)**：让 `resolve_merged_slot` 对**所有**冷函数返回 `None`,只有 `jit_call` 有健壮兜底;
`jit_vcall`/`jit_call_indirect`/`jit_obj_new` 的 `None`-臂**对任意冷函数不健壮**（改前 `None` 只意味罕见
"不可编"）→ 86 个 jit golden 挂（`CallIndirect: undefined function lambda`、字段值 5→0 等）。
**收窄(Phase 1a,本次)**：阈值只作用于 `jit_call`（静态/自由调用,`cross_zpkg_via_interp` 已证通用）——
新增 `resolve_fn_by_id_tiered`,仅 `jit_call` 用;方法/闭包/构造(vcall/indirect/objnew)保持 compile-on-first-call
（非 tiered `resolve_fn_by_id`,行为不变、安全）。**Phase 1b**:让这三个 helper 的 `None`-臂也健壮 interp
任意冷 callee,再把它们切到 tiered（届时静态验证 + 结果一致测试）。三态负缓存两条路径通用。

### Decision 4：阈值可配,默认基准定
`Z42_JIT_THRESHOLD`（host config）。N=1 = 现状（首 call 即编,无分层）;N>1 滤冷函数。默认先取中间值(建议 N=2 起步,
基准后调)。**语义**:第 N 次调用时编译,前 N-1 次 interp。

### Decision 5：Phase 1.5 混合模式 = Phase 2 回收的安全前提
回收 `Function.blocks` 只有在"该函数永不被 interp 执行"时才安全。当前 interp 的 Call 永远留 interp → 已编译函数
仍可能被 interp 帧调用 → 不敢回收。**Phase 1.5** 让 interp 的 Call/VCall 也查 `FnEntry`（Compiled → 原生）→
已编译函数永不被 interp 执行 → blocks 可安全回收。**故 Phase 2 依赖 Phase 1.5**。

### Decision 6：Phase 2 只回收 blocks + 池化（准则 2 第 4 条）
- 只 drop `blocks`（指令体,内存大头）;**保留** `exception_table`/`line_table`/`reg_types`/`name`/`param_count`/
  `max_reg`（栈迹/catch/frame setup 仍需）。
- **所有权粒度**:`Function.blocks` 从裸 `Vec` 改为可单独释放的容器（`Mutex<Option<Vec<BasicBlock>>>` 或等价），
  脱离"整个 `Vec<Function>` 一个 `Arc`"无单函数释放句柄的现状。
- **池化**:回收的 `Vec<BasicBlock>`/`BasicBlock` 进模块/线程级 free-list,下次加载或(未来 deopt)重用容量,
  **不回收即还 OS**。镜像 `REGS_POOL`/`FRAME_POOL`。

### Decision 7：单向分层,不做 deopt
只 interp→JIT 升级,不做 JIT→interp 逆向 deopt（无 speculative 假设需要回退）。简化生命周期:Compiled 是终态。
→ 回收 blocks 后无需重建（除非未来引入 deopt,那时从 free-list/重编重建,故池化保留容量有意义）。

## Implementation Notes
- 计数溢出:`AtomicU32` 到阈值即停增（`fetch_add` 后不再关心具体值,或 saturating）。
- Rejected 与 lazy IC:`call_jit_ic`/`cross_module_targets` 需与三态一致(Rejected 目标 IC 记 interp 路由)。
- Phase 1 不碰 interp 侧(interp 模式行为不变);只改 jit 模式的 callee 分发。

## Testing Strategy
- Phase 1:①冷函数(<阈值)走 interp 且结果正确 ②到阈值编译 + 之后原生 ③不可编函数记 Rejected 不重扫(计数/日志验)
  ④冷跑 interp 结果 == 升级后原生结果(同函数两路一致)⑤全 GREEN(e2e interp+jit + 自举 + stdlib)⑥基准:多冷函数
  程序编译时间下降 + 热循环不回归。
- Phase 1.5:interp 帧调用已编译函数走原生(计数/perf 验) + 全 GREEN。
- Phase 2:回收后内存下降(RSS/计数) + 无 OS alloc 抖动(池命中率) + 全 GREEN + GC-stress(回收与 GC 不冲突)。

## Deferred / Future Work
- deopt（JIT→interp）；AOT；跨线程 code cache 共享。
