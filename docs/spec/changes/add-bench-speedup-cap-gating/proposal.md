# Proposal: bench jit/interp 加速比派生指标 + 场景能力门控

## Why

`add-exec-profile-matrix`（PR #83）让 bench 同时测 interp 与 jit，**目的就是量化「JIT 比 interp
快多少」**——但落地时漏了两块：

1. **加速比未落地**（原设计 Decision 6 / task 2.5）：`bench --diff` 现在分别列出 interp 与 jit 两条
   数字，却从不计算二者的**加速比**。整套双模式测量的 payoff（`jit vs interp: 2.3× faster`）缺失。
2. **场景无能力门控**：`06_thread_scaling` 需要 `threads` 能力；在无 threads 的 VM（wasm / 未来平台
   bench）上会直接崩。当前 e2e bench 无差别跑所有场景，不看场景对能力的要求。

## What Changes

1. **加速比派生行**：`bench --diff` 对同一 `(name, metric, platform)` 下**同时有 interp 与 jit**
   的两条结果，额外派生一行 `<name> [platform]  interp/jit 加速比: N.Nx`（interp.value /
   jit.value；>1 = jit 更快）。纯派生展示，不入 schema、不触发回归判定。
2. **场景能力门控**：scenario 顶部可声明 `// requires-caps: <csv>`（如 `threads`）。e2e `_bench`
   探针拿到被测 VM 的 caps 后，对每个场景检查其 requires-caps ⊆ vmCaps.caps；缺任一 → **显式跳过 +
   打印原因**（不静默、不崩）。`06_thread_scaling` 加 `// requires-caps: threads`。

## Scope（允许改动的文件）

> 占用子系统：`toolchain`（scripts/）。`bench/`、`docs/` 不上锁。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/xtask_bench.z42` | MODIFY | `_benchDiff` 加加速比派生行；`_bench` 加 requires-caps 门控 |
| `scripts/common/xtask_exec_profile.z42` | MODIFY | 加 `_epScenarioRequiredCaps`(解析注释) + `_epCapsMissing`(caps 子集检查) 助手 |
| `bench/scenarios/06_thread_scaling.z42` | MODIFY | 顶部加 `// requires-caps: threads` |
| `bench/README.md` | MODIFY | 记加速比行 + requires-caps 约定 |
| `docs/design/testing/exec-profile-matrix.md` | MODIFY | §3 门控 + §5 加速比补记 |

**只读引用**：`scripts/xtask_bench.z42` 现 `_extractNamespace`（注释解析范式参照）。

## Out of Scope

- 不改 schema（加速比是派生展示，不落 profile/tier）。
- 不做平台 bench（仍 Deferred）；本门控只是让「无能力场景」在任意 VM 上安全跳过，为平台 bench 铺垫。

## Open Questions

- 无（沿用已归档 exec-profile-matrix 的既定设计；本change 是其两处补齐）。
