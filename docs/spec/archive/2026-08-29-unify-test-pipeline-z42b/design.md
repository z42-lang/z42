# Design: 统一测试流水线归 z42b

## Architecture

```
xtask（语料/fleet 层 —— repo 结构知识，留在 harness）
  ① 发现语料(src/tests/**、libraries/<lib>/tests/**) + 编译全部 → .zbc
  ② 聚合成 bundle manifest {cases:[golden|unit …]}
        │ 调用（每平台一次）
        ▼
z42b test [--rid <platform>]（单目标层 —— 执行器）
  host(默认)   → builder_test.z42 进程内反射跑
  --rid mobile → 确保 test workload + 平台 workload 就位
               → 打包 + 部署 bundle + 嵌入 agent
        ▼
test-agent(on-device，workload/test/agent)→ Std.Test.Runner → JSON 报告回收
```

## Decisions

### D1: 两层职责切分（z42b 单目标 vs xtask 语料）

**问题：** z42b 输入是一份 z42.toml（单项目/单 target）；repo 测试语料是散装 fixture
（无 per-case toml），天然「多工程/无工程」。
**选项：** A—xtask 保持顶层编排、z42b 当单目标执行器；B—z42b 吞掉语料 sweep。
**决定：** A。语料发现/编译/分片/聚合/门禁是 repo 结构知识，属 dev harness，不属「一个项目的
构建」；强塞进 z42b 会让它背上非项目职责（违设计完整性）。

### D2: bundle manifest 作为两层的缝

**问题：** xtask 的「多用例」如何喂给 z42b 的「单目标」执行器？
**决定：** 复用现有 bundle 契约——`agent._runBundleReport` 已吃 `{cases:[…]}`（golden 隔离 VM +
unit 共享 VM）。xtask 编完语料吐 bundle，`z42b test --rid` 消费。缝已存在，无需新协议。

### D3: agent 载荷归位 test 能力 workload，产物名不变

**问题：** testhost 独立目录承载一份平台无关小 payload，语义碎；想简化 + 明确分发模型。
**选项：** A—`builder/testagent`（归 z42b，随 SDK 核心恒在）；B—`workload/test/agent`（归能力
workload，按需下载）。
**决定：** B。**跑测试是按需能力，不是恒需**——正是 workload 的 on-demand 模型（`z42 workload
install test`）。z42b 保持纯编排器、**不夹带无关 zpkg**；要跑测试时拉取并调用 test workload。
源码迁 `src/toolchain/workload/test/agent/`，取消 testhost 目录。产物 `z42.testagent.zpkg` 名
**保持不变**——4 平台 host 按 deploy-relative `app/z42.testagent.zpkg` 运行期加载，与源码位置
解耦，故零运行期改动。
**内聚代价认知：** agent 的消费者（各平台嵌入 host）在 `workload/<plat>/`，共享 agent 在
`workload/test/`——嵌入 harness 被拆成「共享半 + 平台半」，这**正确映射了共享 vs 平台的分发边界**
（平台 host 随各平台 workload 下载，共享 agent 随 test workload 下载）。

### D4: host 与 on-device 统一为 `z42b test [--rid]`

**问题：** 两个测试前端心智模型割裂。
**决定：** 统一 verb：无 rid = host 进程内（builder_test）；有 rid = deploy+agent。同一执行器两条
尾分叉，对齐 z42b「一次产平台无关件、export/test 才分叉」立柱。（本 change 只定形，不接线——见 D5。）

### D5: 分阶段落地（尊重 wire-z42b-host-build 时序）

- **阶段 1（本 change）：** 载荷归位 + 架构定稿。xtask 仍编译语料 + 驱动嵌入运行。零 CLI/打包改动。
- **阶段 2（wire-z42b-host-build 后）：** z42b 接管 ③（deploy+run 单 bundle/项目）+ test workload
  打包/发布落地（见 D6）。**核心目标 = 简化 xtask→z42b 调用**：`xtask_test_platform.z42` 各
  backend（wasm/ios/android/desktop）当前自带一套「怎么 build/deploy/run 这一份」的 bespoke 逻辑
  （`IPlatformBackend.BuildProject/Assets/RunTests`），阶段 2 全部**委托给 `z42b test --rid`**，
  backend 收缩为「声明 rid + 转调 z42b + 翻译报告」的薄壳，消除四平台重复。xtask 只保留语料级
  编排（发现/编译/分片/聚合/门禁），单目标的 build/deploy/run 一律下沉 z42b。
- **阶段 3（z42b in-process 编译成熟）：** ② 中「编译一个项目」走 z42b；但**语料级编译仍留 xtask**
  （非项目）。

### D6: test workload 是「payload-only」新形状（install CLI 无需改，打包侧后续轻量扩展）

**事实：** `z42 workload install <wl>` 的 `<wl>` 为 manifest 驱动的自由名（查 `release-index.json`），
host 门控已支持 `host:["*"]`（desktop 即是）。→ **install 命令面无需改**，`z42 workload install test`
语法即成立。
**新形状：** 现有 workload = 「平台 tooling + per-RID runtime pack」（`_workloadPkgHeader` →
`kind="workload-tooling"` + per-RID native build）。test workload 是**纯 payload**：只有 agent
zpkg、无 per-RID native pack、`host:["*"]`。
**后续阶段需做（非本 change）：** ① 打 payload-only workload archive；② `release-index.json` 加
`test` 条目（`host:["*"]`、`runtimes` 空）。运行侧：`z42b test --rid <p>` 确保 test workload + 平台
workload 两者就位。
**Open（接线阶段定）：** 复用 `kind="workload-tooling"`（runtimes 留空）还是加 `kind="workload-payload"`。

## Implementation Notes

- 迁移 = `git mv` agent 两文件到 `workload/test/agent/` + 改 toml 内 include 相对路径 +
  改 `scripts/test/xtask_test_embedded.z42` 第 19–20 行（toml/dist 路径）。产物名/命名空间（本步）不动。
- 验证仍走 `xtask test embedded`（desktop 嵌入 harness）+ platform 各 backend。
- 平台 host（swift/kotlin/c/js）按 `app/z42.testagent.zpkg` 加载，**不改**。

## Testing Strategy

- 回归：`xtask test embedded`（desktop/wasm）必须仍绿——证明迁移未断构建接线。
- 平台（可跑时）：`xtask test platform desktop` 端到端跑一个用例。
- 完整 GREEN：`xtask test`（全 stage）——确认 compiler/stdlib 不受影响。
- 文档：embedded-app-run.md / cross-platform-testing.md 路径与架构一致，无死链。
