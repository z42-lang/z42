# Proposal: 统一测试流水线归 z42b（第一步：载荷归 test workload + 目标架构定稿）

## Why

测试的「跑」现在有两个前端——z42b `builder_test`（host 进程内反射跑）与 testhost `agent`
（on-device 嵌入跑）——外加 xtask 三相编排，「编译→部署→运行」的完整流程散在
xtask + testhost + workload 三处，没有单一 owner。testhost 又作为独立顶层目录承载一份很小的
平台无关 payload，目录语义偏碎。需要确立清晰两层：**z42b = 单目标测试执行器**（host 与
on-device 统一），**xtask = 语料级编排器**，二者以 bundle manifest 为缝；并把 on-device
test-agent 归位到一个**按需下载的 test 能力 workload**，取消独立 testhost 目录。

## What Changes

- 落文档定稿目标架构：z42b 拥有「编译→部署→运行**一个**目标（项目 or bundle）」的统一执行器
  职责；xtask 保持「语料发现/编译全量/分片/聚合/门禁」的编排职责；缝 = 现有 bundle manifest。
- 把 on-device test-agent 源码从独立 `src/toolchain/testhost/` 迁入**能力 workload**
  `src/toolchain/workload/test/agent/`，取消 testhost 顶层目录。
- 产物 zpkg 名 `z42.testagent.zpkg` **保持不变** → 4 平台运行期加载路径零改动。
- 本步**不**碰 install CLI、**不**碰打包/分发、**不**接通 z42b 的 in-process 编译/部署
  （依赖 wire-z42b-host-build）。xtask 仍经 z42c 编译语料并驱动嵌入运行。纯归位 + 架构定稿。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/workload/test/agent/src/agent.z42` | NEW | 由 testhost 迁入（内容不变） |
| `src/toolchain/workload/test/agent/z42.testagent.z42.toml` | NEW | 迁入 + 修正 include 相对路径 |
| `src/toolchain/workload/test/README.md` | NEW | 六段制；定位「跑测试流程时按需下载的能力 workload」 |
| `src/toolchain/testhost/agent/src/agent.z42` | DELETE | 迁出 |
| `src/toolchain/testhost/agent/z42.testagent.z42.toml` | DELETE | 迁出 |
| `src/toolchain/testhost/README.md` | DELETE | 目录取消 |
| `scripts/test/xtask_test_embedded.z42` | MODIFY | 第 19–20 行 toml/dist 路径 → `workload/test/agent` |
| `src/toolchain/workload/README.md` | MODIFY | 登记 test 能力 workload；澄清「now 也含一个平台无关共享能力束」 |
| `docs/design/testing/embedded-app-run.md` | MODIFY | impl 路径 testhost→workload/test/agent；补统一流水线框架 |
| `docs/design/testing/cross-platform-testing.md` | MODIFY | 落「两层模型 + bundle 缝 + 分阶段 + test workload 形状」核心设计 |
| `docs/roadmap.md` | MODIFY | 登记本 change + 后续阶段（z42b 接管 ③/②、payload workload 打包）索引 |
| `.github/workflows/ci.yml` | MODIFY | paths-filter 删 stale `src/toolchain/testhost/**`（新位置已被同组 `workload/**` 覆盖）—— 实施期发现，2026-08-29 扩 Scope |

**只读引用**（理解上下文，不改）：

- `src/toolchain/builder/core/builder_test.z42` — host 侧现有 test verb
- `src/toolchain/launcher/core/launcher_workload.z42` — 证 install CLI 为 manifest 驱动、`host:["*"]` 已支持
- `scripts/package/xtask_package.z42` — 证现有 workload 打包为「平台 tooling + per-RID pack」形状
- `src/toolchain/workload/{wasm,ios,android,desktop}/…` — 运行期按 zpkg 名加载，本步无需改
- `src/libraries/z42.test/src/Runner.z42` / `ModuleLoader.z42` — 共享 runner 核心

## Out of Scope

- 接通 z42b in-process 编译/部署（② / ③ 真正移交）——依赖 wire-z42b-host-build，后续 change。
- `z42b test --rid` CLI 的实际接线——本步只定架构，不加 verb。
- test workload 的**打包/发布/manifest 条目**（payload-only workload 形状）——后续接线阶段。
- install CLI 改动——经核实**无需改**（manifest 驱动 + `host:["*"]` 已支持）。
- workload B 阶段扁平化重构——正交，另 change。
- agent namespace 是否由 `Z42.TestHost.Agent` 改名——见 Open Questions。

## Open Questions

- [ ] agent namespace 保留 `Z42.TestHost.Agent` 还是随迁改（如 `Z42.Workload.Test.Agent`）？
      pre-1.0 可改，但要 grep 确认无外部引用；**倾向本步保留**以缩小半径，改名单列。
- [ ] payload-only workload 的打包路径：复用现有 `kind="workload-tooling"` 壳（`runtimes` 留空）
      还是加 `kind="workload-payload"`？——留给接线阶段定，本步不决（记入 design D6）。
