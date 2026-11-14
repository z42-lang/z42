# GitHub Actions Workflows

## 职责

仓库的 CI/CD 自动化配置。每个 `.yml` 文件定义一个独立 workflow；可复用步骤在 `../actions/`
（`ci-bootstrap`、`setup-z42-sdk`、`xtask-bootstrap-artifact`），`../ci/xtask` 是 CI 里调用 `xtask` 的 shim。
不写业务逻辑——构建 / 测试 / 打包都经 `xtask <cmd>` 调用。

## 当前 workflows

| 文件 | 触发条件 | 职责 |
|------|---------|------|
| `ci.yml` | `pull_request` / `push` 到 main；`workflow_dispatch`；每日 16:00 UTC `schedule` | 主门禁 + nightly 发布，job 见下表 |
| `retry-ci-runner.yml` | PR CI 请求 / 失败后；`workflow_dispatch` 指定 run ID | 请求时清理同 PR 的过期运行；失败时只对未分配到 hosted runner、没有执行步骤的故障重试，最多两次；保留 `ci-ok` 门禁 |
| `bench-pr.yml` | `pull_request` 到 main（仅 runtime / libraries / compiler / bench / xtask 脚本路径，纯 `.md` 不触发）；`push` main 与每周一 `schedule` 仅预热缓存；`workflow_dispatch` | `bench-regression(linux-x64)`：同 runner A/B，`xtask bench --ab --tier gate --threshold-time 0.15`（初判回归的条目再测 2 轮、用跑间离散度重算区间），>15% 时间回归且区间分离即 fail；micro 层同为硬门禁（base / pr 各出 `xtask bench stdlib --json`，`xtask bench --micro-diff --threshold-time 0.15` 判红）；criterion 层只打印。`bench-cache-warm`：main 上预热 Rust 依赖缓存。判红语义与阈值依据见 internals「性能基准与回归门禁」 |
| `release.yml` | 推 tag `v*.*.*`；`workflow_dispatch`（输入 version） | `verify-version`（tag ↔ `scripts/versions.toml`）→ `package-<rid>` × 9 个 RID → `publish-release`（含 SHA256SUMS） |
| `deploy-book.yml` | `push` main 且改动书 / examples / 安装脚本 / stdlib 源码 / doc 渲染器；`pull_request`（只构建，`[ERROR]` 判红）；`workflow_dispatch` | `xtask build docs` 现生成 reference 的 stdlib 包页，构建 mdBook 并发布到 GitHub Pages（含 `install-z42.sh` / `install-z42.ps1`） |
| `jit-fixpoint-check.yml` | 仅 `workflow_dispatch`（实验性，不进门禁） | 逐平台验证 z42c 工作区 `--mode jit` 与 `--mode interp` 产物逐字节一致 |

### ci.yml 的 job

| job（显示名） | 何时跑 | 职责 |
|------|------|------|
| `changes`（detect-changes） | 总是 | 按路径分类，并输出下游测试和打包任务的选择结果 |
| `docs-check` | `docs_checks`，且未由宿主门禁覆盖 | `xtask test docs links` 死链门禁 + `xtask check diagcodes` + `xtask check layout` |
| `build-and-test`（test-host） | `host_checks` | 多平台 `xtask ci test-host --plan` |
| `host-package`（package-host） | platform / sdk_checks / examples 验证，或有发布输入，或定期 / 手动运行 | 各 host RID 的 SDK 打包与验证。平台打包时还构建 runtime / workload |
| `toolchain-bootstrap`（compile-toolchain） | `toolchain_build` | 编译 toolchain（z42c / stdlib / xtask zpkg）供下游 job 消费 |
| `assemble-current-sdk` / `consume-current-sdk` | `test_assets` | 组装当前 SDK 与共享 golden 产物，再下载执行语言和标准库 golden |
| `vm-jit-consistency` / `stdlib-consistency` | 分别读取 `vm ‖ compiler` / `stdlib` | VM / 标准库在 JIT 与 interp 下的一致性，分 shard |
| `compiler-checks` | compiler / 专项执行器改动或 schedule / dispatch | 编译器专项检查 |
| `package-ios` / `package-android` / `package-wasm` | 对应执行器、platform 或累计发布验证 | 移动 / wasm 平台包 |
| `test-wasm` / `test-ios` / `test-android` | 对应执行器改动跑 smoke；schedule / dispatch 跑全量 | Tier 2 平台测试（浏览器 / iOS 模拟器 / Android 模拟器），分 shard |
| `test-desktop` | platform 改动或 schedule / dispatch | 桌面 C ABI 测试 |
| `publish-nightly` | push / dispatch 到 main | 汇总各 RID package → 覆盖 `nightly` GitHub Release（prerelease，URL 稳定） |
| `ci-ok` | 总是 | 聚合门：所有 job 成功或 skipped 才绿（分支保护只认它） |

## 设计约定

- **用例归属与选测**：新增用例按[测试用例组织规范](../../docs/internals/src/devinfra/test-layout.md#1-用例放哪)选择目录、形态和本地执行器；修改 workflow 时分别核对[构建前提、测试与发布边界](../../docs/internals/src/devinfra/ci.md#构建测试与发布的选择边界)，工程归属与用例能力不在 YAML 里另抄一份名单。
- **统一入口**：workflow 只调 `xtask <cmd>`，不在 yaml 里硬编码构建 / 测试命令。
- **空 PR**：真实 Git 差异为空时不申请宿主、工具链或测试资产，仅保留轻量检测和 `ci-ok`。有改动的 draft PR、全量回退与累计 SDK 发布继续验证。
- **性能类别**：`flows.toml` 分开选择脚本 micro、Rust VM、集成 A/B；只选 VM 时不构建 z42 工具链。
- **测试基线**：PR 使用 PR base，main push 使用事件的 `before`；初次 push、无法验证基线、定期 / 手动运行保留全量。基线选择与回归在 [流程规划](../../docs/internals/src/devinfra/flows.md)，发布仍使用 nightly 的累计 SDK 差异。
- **工具链私有测试**：builder、interactive、launcher 的私有 `tests/` 由宿主测试或 SDK 验证消费，路径筛选从 `platform` 排除这些目录。builder 的私有测试也从 `stdlib` 排除。混合改动逐文件判定，生产输入仍选择对应专项。
- **SDK 验证**：`scripts/src/test/sdk/` 与 launcher 的发行包私有夹具只选择四个桌面 host package；打包生产实现继续选择设备 / 浏览器消费者，无需逐用例平台配置。
- **Runtime 私有测试**：顶层 Rust 测试与已确认的纯测试数据由四个平台的完整 Rust harness 覆盖；`runtime_checks` 保留它们，`vm` / `platform` 排除它们。build.rs 使用的 `numz42-c`、未知数据、生产输入、混合改动及定期全量继续保留消费者。
- **自举**：CI 从上一个 nightly（取不到则回退 main 上最近成功 CI 的产物）的 SDK 起步，自己编当前源码；机制见 [CI 拓扑 · 自举种子从哪来](../../docs/internals/src/devinfra/ci.md)。
- **缓存**：Rust 依赖用 Swatinem `rust-cache`，bench 缓存由 main 上的 `bench-cache-warm` 预热。
- **VM 准备**：共享工具链 action 默认构建 release VM，可复用同轮 Linux x64 产物，也可交给测试命令构建所需 profile。JIT golden 分片使用第三种方式构建 debug VM。[CI 拓扑](../../docs/internals/src/devinfra/ci.md)
- **矩阵失败**：PR 上同矩阵的一项失败会取消其余项；main / 定期 / 手动运行保留完整结果。独立 job 与不同 workflow 不由矩阵 fail-fast 联动。
- **并发控制**：PR 上同一 ref 的旧 run 自动取消（`concurrency` 段）；main 的 push 运行不自动取消，由 `xtask ci supersede` 只取消不影响 SDK 的旧运行。可信 main 的控制 workflow 正常取消同 PR 的旧 head 后，等待并重新校验，再强制结束仍滞留的旧运行。job 由 GitHub 原生队列按可用 runner 调度。
- **macOS 容量与恢复**：主 CI 通过 `xtask ci queue` 为 macOS 分配 PR 两槽、后台一槽的独立有界资源池；runner 未取得时的有限重试由 `xtask ci retry-runner` 判定，规则见 [CI 拓扑](../../docs/internals/src/devinfra/ci.md#macos-资源队列与-runner-获取失败恢复)。

## 如何测试验证

CI 只能在 GitHub 上跑；本地对应的门禁是 `xtask test docs links`（文档）与 `xtask test`（全量）。改动 workflow 后以 PR 上 `ci-ok` 为绿为准。

夜间资产上传由 `xtask ci upload-release` 逐项执行，核对远端摘要、大小与状态；瞬时错误做有界退避重试。
权限错误及重试耗尽仍让发布失败。runner 恢复执行可信 main 的 xtask，不检出或运行 PR 代码。

`xtask check flows` 在本地与 detect-changes 中执行配置、选路、Git、恢复、上传、自举、golden 与安装器回归。
目录规则与依赖图统一配置在 `scripts/flows.toml`。计划通过 artifact 共享，host 与 compiler jobs 校验后消费，
详细接口见[流程规划](../../docs/internals/src/devinfra/flows.md)。

## 关联文档

- CI 设计与 job 说明：[ci.md](../../docs/internals/src/devinfra/ci.md)
- 发版流程：[release.md](../../docs/internals/src/devinfra/release.md)
- 性能门禁：[benchmarking.md](../../docs/internals/src/devinfra/benchmarking.md)
