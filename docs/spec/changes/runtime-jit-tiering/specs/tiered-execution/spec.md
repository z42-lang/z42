# Spec: 分层执行（tiered execution）

## ADDED Requirements

### Requirement: 阈值分层（Phase 1）

jit 模式下,函数不再首次调用即编译;调用计数达阈值前走 interp,达阈值才编译缓存。

#### Scenario: 冷函数不编译
- **WHEN** 一个可 JIT 函数被调用次数 < 阈值
- **THEN** 经 interp 执行(cold tier);其 `FnEntry` 槽保持 Unknown(未编);`jit_methods_compiled` 不增

#### Scenario: 热函数到阈值升级
- **WHEN** 同一函数第 N(=阈值)次被调用
- **THEN** 编译并缓存原生码;此后调用走原生;`jit_methods_compiled` +1

#### Scenario: 冷跑与升级后结果一致
- **WHEN** 同一函数先冷跑(interp)后热升级(原生)
- **THEN** 两路对同一输入产出相同结果(分层不改变语义)

### Requirement: 三态负缓存（Phase 1）

不可 JIT 的函数一次判定后记 Rejected,不再每次调用重扫。

#### Scenario: 不可编函数记负缓存
- **WHEN** 一个含 interp-only opcode(如 CallNative)的函数达阈值
- **THEN** 其槽记 Rejected(`FnEntry.ptr==null`);后续调用直接走 interp,**不再重跑 `jit_unsupported_reason` 扫描**

### Requirement: 混合模式（Phase 1.5）

interp 的 Call/VCall 分发能路由到已编译原生码。

#### Scenario: interp 帧调用已编译函数走原生
- **WHEN** 一个 interp 执行中的函数调用另一个已 Compiled 的函数
- **THEN** 该调用走原生码(非 interp);已编译函数不被 interp 执行

### Requirement: IR 回收 + 池化（Phase 2）

已编译且不再被 interp 执行的函数,回收其 `blocks`;回收内存池化复用。

#### Scenario: 回收已编译函数的 blocks
- **WHEN** 函数 Compiled 且 Phase 1.5 保证其永不被 interp 执行
- **THEN** 其 `Function.blocks` 被回收(内存下降);`exception_table`/`line_table`/`reg_types` 保留(栈迹/catch/frame 仍正确)

#### Scenario: 回收走池化不抖动 OS
- **WHEN** 大量函数回收 blocks
- **THEN** 回收的容器进 free-list 复用,不逐个还给 OS;OS 内存申请/归还次数不随回收数线性增长

## VM Mapping
- 分发:`jit/frame.rs` resolve + `jit/helpers/call.rs` jit_call(Phase 1);`interp/exec_call.rs`/`exec_vcall.rs`(Phase 1.5)
- 计数/槽:`JitModuleCtx.call_counts` + 三态 `FnEntry`
- 回收:`Function.blocks` 所有权粒度 + free-list 池(Phase 2)

## Pipeline Steps
- [x] 分发决策(interp/jit) — Phase 1 改 jit callee 分发
- [x] JIT 缓存(三态) — Phase 1
- [x] 内存生命周期 — Phase 2
