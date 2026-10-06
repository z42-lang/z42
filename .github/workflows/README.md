# GitHub Actions Workflows

## 职责

仓库的 CI/CD 自动化配置。每个 `.yml` 文件定义一个独立 workflow；可复用步骤在 `../actions/`
（`ci-bootstrap`、`setup-z42-sdk`、`xtask-bootstrap-artifact`），`../ci/xtask` 是 CI 里调用 `xtask` 的 shim。
不写业务逻辑——构建 / 测试 / 打包都经 `xtask <cmd>` 调用。

## 当前 workflows

| 文件 | 触发条件 | 职责 |
|------|---------|------|
| `ci.yml` | `pull_request` / `push` 到 main（只改 `.claude/**` 不触发）；`workflow_dispatch`；每日 16:00 UTC `schedule` | 主门禁 + nightly 发布，job 见下表 |
| `bench-pr.yml` | `pull_request` 到 main（仅 runtime / libraries / compiler / bench / xtask 脚本路径，纯 `.md` 不触发）；`push` main 与每周一 `schedule` 仅预热缓存；`workflow_dispatch` | `bench-regression(linux-x64)`：同 runner A/B，`xtask bench --ab --tier gate --threshold-time 0.25`，>25% 时间回归且区间分离即 fail；micro 层只打印不判红。`bench-cache-warm`：main 上预热 Rust 依赖缓存。阈值依据见 internals「性能基准与回归门禁」 |
| `release.yml` | 推 tag `v*.*.*`；`workflow_dispatch`（输入 version） | `verify-version`（tag ↔ `scripts/versions.toml`）→ `package-<rid>` × 9 个 RID → `publish-release`（含 SHA256SUMS） |
| `deploy-book.yml` | `push` main 且改动书 / examples / 安装脚本；`pull_request`（只构建，`[ERROR]` 判红）；`workflow_dispatch` | 构建 mdBook 并发布到 GitHub Pages（含 `install.sh` / `install.ps1`） |
| `jit-fixpoint-check.yml` | 仅 `workflow_dispatch`（实验性，不进门禁） | 逐平台验证 z42c 工作区 `--mode jit` 与 `--mode interp` 产物逐字节一致 |

### ci.yml 的 job

| job（显示名） | 何时跑 | 职责 |
|------|------|------|
| `changes`（detect-changes） | 总是 | 按路径过滤出 platform / examples / compiler / vm / stdlib / docs_only，给下游 `if:` 用 |
| `docs-check` | 仅文档改动 | `xtask test docs links` 死链门禁 + `xtask check diagcodes` |
| `build-and-test`（test-host） | 非纯文档 | 多平台 `xtask build` + `xtask test` |
| `host-package`（package-host） | platform / examples 改动或非 PR | 各 host RID 的 package 产物 |
| `toolchain-bootstrap`（compile-toolchain） | 非纯文档 | 编译 toolchain（z42c / stdlib / xtask zpkg）供下游 job 消费 |
| `assemble-current-sdk` / `consume-current-sdk` | 随 CI | 当前源码组装 SDK、再消费它跑测试 |
| `vm-jit-consistency` / `stdlib-jit-consistency` / `stdlib-interp-consistency` | vm / stdlib / compiler 改动或 schedule / dispatch | VM / 标准库在 JIT 与 interp 下的一致性，分 shard |
| `compiler-checks` | compiler 改动或 schedule / dispatch | 编译器专项检查 |
| `package-ios` / `package-android` / `package-wasm` | platform 改动或非 PR | 移动 / wasm 平台包 |
| `test-wasm` / `test-ios` / `test-android` | 仅 schedule / dispatch | Tier 2 平台测试（浏览器 / iOS 模拟器 / Android 模拟器），分 shard |
| `test-desktop` | platform 改动或 schedule / dispatch | 桌面 C ABI 测试 |
| `publish-nightly` | push / dispatch 到 main | 汇总各 RID package → 覆盖 `nightly` GitHub Release（prerelease，URL 稳定） |
| `ci-ok` | 总是 | 聚合门：所有 job 成功或 skipped 才绿（分支保护只认它） |

## 设计约定

- **统一入口**：workflow 只调 `xtask <cmd>`，不在 yaml 里硬编码构建 / 测试命令。
- **自举**：CI 从上一个 nightly（失败则回退最近正式 release）的 SDK 起步，自己编当前源码；约束见 [bootstrap-seed.md](../../docs/agent/rules/bootstrap-seed.md)。
- **缓存**：Rust 依赖用 Swatinem `rust-cache`，bench 缓存由 main 上的 `bench-cache-warm` 预热。
- **并发控制**：同一 ref、同一事件类型的旧 run 自动取消（`concurrency` 段）。

## 如何测试验证

CI 只能在 GitHub 上跑；本地对应的门禁是 `xtask test docs links`（文档）与 `xtask test`（全量）。改动 workflow 后以 PR 上 `ci-ok` 为绿为准。

## 关联文档

- CI 设计与 job 说明：[ci.md](../../docs/internals/src/devinfra/ci.md)
- 发版流程：[release.md](../../docs/internals/src/devinfra/release.md)
- 性能门禁：[benchmarking.md](../../docs/internals/src/devinfra/benchmarking.md)
