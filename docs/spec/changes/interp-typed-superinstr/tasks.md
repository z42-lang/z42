# Tasks: 解释器类型化超级指令（Lever 2）

> 状态：🟡 进行中 | 类型：perf | 创建：2026-08-01
> 提案见 [`proposal.md`](proposal.md)。扩展 #93 的超级指令框架（`metadata/superinstr.rs`）。

## 设计要点

### unchecked 提取的安全契约
`Value::as_i64_unchecked` = `match self { I64(x)=>*x, _=>unreachable_unchecked() }`。仅在
`reg_types[r]==I64` 时调用——与 JIT 的 raw-slot 特化**同一不变量**（`translate.rs` 的
`is_i64_typed`）。`debug_assert!(matches!(self, I64(_)))` 在 debug 抓违约；release 无分支。
reg_types 若骗人 = UB，但那等价于 JIT 早已依赖的同一 bug，非本 change 新增风险面。

### 类型化 CmpBr
`SuperInstr::CmpBr` 加 `typed: bool`；`recognize` 在 `reg_types[a]&&reg_types[b]` 均 I64 时置真。
interp 消费：typed 走 `eval_cmp_i64_unchecked`（无 `Result`、无 tag 分支），否则原 `eval_cmp`。

### 算术链融合
块内连续算术 `t=a op1 b; d=t op2 c`，中间 `t` **仅被下一条读**（一次性 per-function reads 扫描
判定单用）→ 融成 `ArithChain`，一步算完、省中间 `Frame::set` + 一次 dispatch。typed（全 I64）时
链内 unchecked。链长先限 2-3。

## 进度概览
- [x] 阶段 1: 类型化 CmpBr（commit 1）
- [~] 阶段 2: 算术链融合 —— **不做**（User 裁决 2026-08-01：结构改动/单用分析风险 vs single-digit% 不值；见备注）
- [x] 阶段 3: 验证 + 文档 + 归档

## 阶段 1: 类型化 CmpBr
- [x] 1.1 `types.rs`：`Value::as_i64_unchecked` / `as_bool_unchecked`
- [x] 1.2 `superinstr.rs`：`CmpBr` 加 `typed`；`recognize`/`compute_fused_tails` 收 `reg_types`（用 `is_integer` 而非 `is_i64`——loop counter 是 I32，narrow int 全存 Value::I64）
- [x] 1.3 `loader.rs`：传 `&func.reg_types`
- [x] 1.4 `ops.rs`：`eval_cmp_i64`（bounds-checked index + unchecked type）
- [x] 1.5 `mod.rs`：typed CmpBr 消费
- [x] 1.6 `cargo test --lib`（854 + 4 recognizer 单测）+ `xtask test e2e --mode interp`（217+8 逐字节一致）

## 阶段 2: 算术链融合 —— 不做（deferred）
> User 裁决（2026-08-01）：通用算术链融合需重构 interp 最热 dispatch 循环（for→while+skip）+
> 新增 per-instruction 融合表 + 单用 reads 分析（分析错=破坏逐字节正确性），~100-150 行在最关键
> 路径，仅换 single-digit%。Lever 1（safepoint inline，2.1×）+ typed CmpBr 已交付主要价值，
> 算术链 ROI 不抵热路径重构风险 → 不做。将来若要，落 `docs/book/` 的 superinstr 页 future 段。
- [~] 2.1–2.4 不做（见上）

## 阶段 3: 验证 + 文档 + 归档
- [ ] 3.1 `cargo test --lib` 全过 + `xtask test`（受 stale-seed 限本地跑 runtime 相关 stage）
- [ ] 3.2 A/B：`Z42_NO_FUSION` 开关测 interp 提速（best of 7）
- [ ] 3.3 `docs/book/src/runtime/superinstr-fusion.md` 加类型化 + 算术链两节
- [ ] 3.4 归档 + commit + PR

## 备注
- 移动端（iOS/Android/WASM interp-only）是主要受益方；桌面 interp 模式同样受益（JIT 模式不走此路）。
