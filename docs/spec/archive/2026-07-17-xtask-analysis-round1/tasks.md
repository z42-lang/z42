# Tasks: xtask-analysis-round1

> 状态：🟢 已完成 | 创建：2026-07-17 | 完成：2026-07-17

**变更说明：** xtask 二次分析（处理流程 / CI / 配置驱动）的**高价值安全项**落地。
**原因：** 三代理分析发现的可安全落地项——① 删 2 处冗余 z42c 自建（处理流程）；② bench-update.yml 缓存错目录（CI）。
**不含**（各有依据）：feature-matrix 并行（会重命名 required check `verify-features(linux-x64)`→踩 phantom-check 坑）；phantom required check `test-compiler-stdlib`（需 User 改 branch protection，我改不了）；RID registry 等结构项（较大、留后续/需决策）。
**文档影响：** 无（纯内部 dedup + CI 缓存；不改命令面/行为）。

## 处理流程：删冗余 z42c 自建（commit 1）
- [x] 1.1 `xtask_test_lib.z42:100` 删 `_buildCompiler()`——`_buildStdlib`（→`_buildStdlibCore`→`_buildCompilerViaZ42c`）已自建 z42c 7 成员 + 自包含 driver；尾随 `_buildCompiler` 是冗余 gen2（fixpoint 保证字节一致）。`_buildRuntime` 保留（format-skew 刷 release z42vm）。**call-graph 已亲验**
- [x] 1.2 `xtask_cli.z42:426`（`build all`）删 `_buildCompiler()`——同上（`_buildStdlib` 已建 z42c）
- [x] 1.3 保留 7 处合法 `_buildCompiler()`（build compiler / test dist / cross-zpkg / vscode / incremental / toolchain-deps / package-desktop——各为独立 standalone，非紧邻 `_buildStdlib` 的冗余）

## CI：bench-update 缓存（commit 2 —— 已实施）
- [x] 2.1 `bench-update.yml` 手写 cache `src/runtime/target` → 换 Swatinem `host-v2`（`.cargo/config.toml` 把 target 重定向到 `artifacts/build/runtime`，现缓存目录是空的 → 每次 push main 冷编 z42vm ~5-8min）

## 阶段 3: 验证
- [x] 3.1 CI 验证 → **compile-toolchain 绿**（run 29518168719，含本轮 commits）。删的 `_buildCompiler` 在 standalone prepare 波（CI 恒 `--no-build` 不走）→ 运行期非 CI 覆盖，靠亲验 call-graph + compile 保证；bench-update 缓存下次 push main 自验
- [x] 3.2 CI 绿后归档 + 释放 toolchain 锁（本次会话）

## 备注
- 直连 main（User 指示）；共享工作树有并行 session WIP，全程显式 `git add`。
- phantom required check `test-compiler-stdlib(linux-x64)` 已实锤（无对应 job）——转告 User 去 GitHub branch protection 移除（+ 审计 verify-selfhost/platform 条件跳过 job 是否也在 required）。
