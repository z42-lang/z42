# 按改动类型的验证速查

> 本地开发时"我改了 X，该怎么验"的查询入口。机制原理见
> [bootstrap.md](bootstrap.md)（自举模型）与 book [测试门禁](../../book/src/dev/test-gate.md)；
> 本页只回答"跑什么、什么顺序、CI 还会替你验什么"。
> 通用前提：**commit 前必须完整 `xtask test` 全绿**——下表的"快速迭代"列不构成 GREEN。

## 速查表

| 改动 | 快速迭代（改一点验一点） | commit 前额外必跑 | CI 替你验的 |
|------|------------------------|------------------|------------|
| **编译器 `src/compiler/`** | `xtask test changed`（→ compiler + vm） | 触及 lexer/parser/codegen/格式 writer，或 z42c 源用了新写法 → `xtask bootstrap-check` | 每腿 ci-bootstrap（种子编当前源）、`verify-selfhost`、JIT 分片腿 |
| **标准库 `src/libraries/`（加 API / 改实现）** | `xtask test changed`（→ lib <lib> + vm）或 `xtask test stdlib <lib> --filter=K` | — | `test-stdlib-jit` 分片、各平台腿 |
| **标准库（删/改 xtask 或 z42c 在用的 API）** | 同上 + 迁移调用点 | ⚠️ **两步舞**：nightly N 先加新 API（旧暂留）→ N 发布后切调用点+删旧。见下"边界为什么管 API" | ci-bootstrap step 2/3（用**种子** stdlib 编 xtask/z42c 源） |
| **VM `src/runtime/`（Rust）** | `cargo test --manifest-path src/runtime/Cargo.toml` + `xtask test vm` | — | 4 OS 腿、JIT 分片、feature-matrix |
| **xtask 源 `scripts/`** | `z42 publish scripts/xtask.z42.toml` 重建 → 随便跑条命令冒烟 | changed 映射对 `scripts/xtask*` = **full**（完整 gate） | ci-bootstrap step 2（种子编 xtask 源） |
| **新语法 / zbc·zpkg 格式** | 阶段一只落 support（仓库源码不用）→ `xtask bootstrap-check` | 格式 bump 另跑 [version-bumping checklist](../../../.claude/rules/version-bumping.md)；等 nightly 发布后才 use | `verify-selfhost` + 全腿 bootstrap；发布死锁自愈见 [ci.md 阶段⑥](../ci.md) |
| **打包 `scripts/package/` / `packages.toml`** | `xtask test packages-config / packages-staging / packages-assemble` | `xtask build package release` + `xtask test dist` | `package-host` + `package-{ios,android,wasm}` |
| **纯文档 / `.claude/`** | 无 | 无（`--scope=docs-only` 零 stage） | 不触发 CI（paths-ignore） |

## 边界为什么管 API（不只是语法）

CI 每条腿冷启动时，**xtask 源和 z42c 源都由"上一 nightly 的种子 z42c + 种子 stdlib"编译**
（`.github/actions/ci-bootstrap` step 2/3）。因此这两个源码域被种子钉死了**两根轴**：

1. **语法/格式轴**：不得用比上一 nightly z42c 更新的语法（[bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md) 的 support-先行纪律）
2. **stdlib API 轴**：不得引用上一 nightly stdlib 里不存在的 API——删改 xtask/z42c 在用的
   API 与用新语法是同一种断链，同样要"晚一个 nightly 再 use"

stdlib 源自身不受种子约束（它由自建的当前 z42c 编译）。

## 验证覆盖矩阵（谁在守哪个格子）

| 源码域 × 工具链 | 种子（上一 nightly） | 当前（本仓） |
|--------|------|------|
| z42c 源 | `bootstrap-check` (A)；CI 每腿 + `verify-selfhost` | gate compiler stage 不动点；`bootstrap-check` (B) |
| stdlib 源 | —（不需要） | gate regen 波 `build stdlib` |
| xtask 源 | ⚠️ 仅 CI（本地无手段） | ❌ 无覆盖 |

> **已知缺口**（2026-07-02 识别，待立项）：① `bootstrap-check` 不编 xtask 源——本地无法提前
> 发现 xtask 越界；② "当前工具链编 xtask 源"处处不验——z42c 变严格 / stdlib 删 API 的破坏
> 会延迟到下一 nightly 变种子后才在 CI 引爆。修复方向：`bootstrap-check` 与 gate 各补一编。
