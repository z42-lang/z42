# Tasks: 惰性加载函数的 token 首执解析（激活 dispatch 缓存）

> 状态：🟢 已完成 | 创建：2026-08-18 | 完成：2026-08-18 | 类型：perf（无可观测行为变化，输出逐字节一致）

**变更说明：** z42 所有 per-site dispatch 缓存（VCall PIC / FieldIC / builtin-id 令牌 /
static-field-id 令牌 / Call 令牌 + cross-module 缓存 / site-index）挂在
`Function.resolved: OnceLock<ResolvedTokens>`，由 `resolver::resolve_module` 填充。但
`resolve_module` **只在 `Vm::run` 里对 entry module 跑一次**——而 interp/JIT 模式下依赖是
**纯惰性加载**（`app.rs` `is_eager = matches!(mode, Aot)`）：除用户 artifact 外所有依赖 zpkg
（自编译时即 z42c.core/syntax/semantics/pipeline 全部）经 `LazyLoader::load_zpkg_file` 进
`function_table`，其 `Function.resolved` **永不被 set**。后果：**整个自编译工作负载（跑在惰性
加载的 z42c.* 里）dispatch 时所有 per-site 缓存全失效**——`resolved == None` → VCall 无 PIC、
Field 无 IC、Builtin/Static 走名字查、每个 Call 对 entry `func_index` 做 String hash+memcmp 且
miss、再 `try_lookup_function`。

**修法：** 把 `resolve_module` 的单函数体抽成 `resolve_function_tokens(func, module, ctx)`，并在
`exec_function_body` 顶部**首次执行时**按需填充（`if func.resolved.get().is_none()`，OnceLock
门禁 → 每函数只解析一次；热路径仅一次 relaxed atomic load，指令循环本就要再读它）。

**模块身份不变式（关键正确性）：** 填充用的 `module` 是该函数运行期实际 dispatch 所对的
Module——始终是 entry module（惰性 callee 由调用方 `module` 透传，根在 entry）。`method_tokens`/
`type_tokens` 是 `module.functions`/`module.type_registry` 的下标，对别的 module 解析会铸错下标；
跨模块目标在此正确解析为 `UNRESOLVED`，交由既有 `cross_module_targets` per-site 首执缓存兜住。
`vcall_ic`/`field_ic`（运行期首派填充）、`builtin_tokens`（全局闭集）、`static_field_tokens`
（全局 `ctx.resolve_static_field_id`，锁保护幂等）均与 module 下标无关 → 本优化主要收益来源。

**原因：** post-#219 profile 里 `get_inner`(210)+`memcmp`(190,FQ 串键比较)+`try_lookup_*` 仍占
~12%——根因非 hash 本身（FxHash #215 已治），而是**惰性代码的 per-site 缓存从未被填充**这一数据
结构接线缺口。

**实测：** 前端 typecheck（`--dump-bound` big.z42，hyperfine -r6）origin/main 6.10s →
**4.84s = 1.26×（-21%）**，σ 0.01–0.03；`--dump-bound` 输出**逐字节一致**。re-profile 确认
`memcmp` 190→37、`get_inner` 210→115、`try_lookup_*` 掉出 top-25。为单一杠杆最大收益（> #219 的 1.19×）。
**无格式/wire/语义变更、无格式 bump。**

**文档影响：** 机制原理（惰性加载 token 解析 + 模块身份不变式 + 并发安全）落
`docs/design/runtime/vm-architecture.md`「惰性加载函数的 token 首执解析」小节（紧接 cross-zpkg
Call 目标缓存章）。

## 任务
- [x] 1.1 `resolver.rs`：抽 `resolve_function_tokens(func, module, ctx)`（`resolve_module` 改为循环调用）
- [x] 1.2 `interp/mod.rs`：`exec_function_body` 顶部 OnceLock-gated 首执解析
- [x] 1.3 `docs/design/runtime/vm-architecture.md`：新增机制小节
- [x] 2.1 `cargo test --release --lib`（926+21 passed, 0 failed）+ `cargo test --release --tests`（集成，全绿；signal_handler_e2e 因缺 example 助手 + 沙箱信号投递环境性挂起，非本改动、不在 xtask gate 内）
- [x] 2.2 `xtask test` 完整 GREEN gate 全绿：e2e interp 248/0 + goldens 248/0 + cross-zpkg + stdlib + z42c[Test] + **自举 5/5 gen1==gen2 逐字节** + vscode-syntax → **✅ GREEN — all stages passed (C#-free)**
- [x] 2.3 correctness：dump-bound 输出逐字节 identical（diff 0）+ A/B 复测 1.26×（hyperfine）

## 备注
- 并发安全：worker 线程可并发首执同一函数 → 各自构建等价 `ResolvedTokens`（`resolve_static_field_id`
  锁保护、按名幂等同值），`OnceLock::set` 取胜者、落败者丢弃无副作用。
- 只填被执行的函数（比「加载时对整个惰性 module 跑 resolve_module」更省 + 天然拿正确 entry-module 身份）。
- Deferred（本次未做，未来可评估）：① 惰性 Call 即便有令牌，`method_token` 恒 UNRESOLVED → 每次仍对
  entry `func_index` 做一次 miss-hash（cross_cell 只省了 `try_lookup_function`）；给 `method_token` 加
  「确定跨模块」哨兵可省这次 miss-hash。② JIT 侧同源缺口（`call_jit_ic`/lazy translate）是否受益需另测。
- 后续大头（profile 印证）：dispatch 主循环（`exec_function_body` 仍 #1）+ frame 管理
  （push/pop/new_from_regs/drop/bzero）——见 [[interp-bigfour-perf-program]] resume。
