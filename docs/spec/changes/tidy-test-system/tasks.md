# tidy-test-system — 测试体系整理

> 类型：`refactor`/`test`（测试基础设施优化，最小化模式，无需规范先行）。
> 一个 change，内部拆多 commit。目标：局部测试提速 + 用例可发现 + 移动/wasm 采样更均匀。

## 目标（User 裁决 2026-08-19）

1. **按目录/用例局部测试、省时间**：执行层过滤已具备（`test e2e --dir/--file`、
   `test stdlib -k`），补齐**构建层最小化**——`--file` 配 `--no-build` 只重编命中用例。
2. **摸清每个用例怎么跑 / 支持平台 / 受哪些影响**：新增只读 catalog 命令 + 文档收敛。
3. **移动/wasm 尽可能多跑用例** → 裁决=**代表性子集（解读 A）**：60-cap 由「字母序前 60」
   改「按类 round-robin 采样」，覆盖更均匀，不动 CI 拓扑。

## 任务 / commit

- [x] **c1** golden regen 支持 dir/file 用例过滤
  - `_collectGoldenCasesF` / `_regenGoldenF` / `_regenForTestF` 加 `dirFilter/fileFilter`，
    复用 `_e2eIncluded` 同款判定；`_testE2eCore` 透传。默认 `"",""` 全量不变。
  - 文件：`scripts/build/xtask_test_assets.z42`、`scripts/test/xtask_test.z42`
- [x] **c2** `xtask test list` 只读 catalog
  - 抽 `_enumerateCorpus`（枚举 SoT）→ `test list` 打印（name/kind/run-cmd/mode/平台门控），
    `--dir/--filter/--kind/--rid/--json`。
  - 文件：`scripts/test/xtask_test_list.z42`(新)、`scripts/test/xtask_test_embedded.z42`、`scripts/xtask_cli.z42`
- [x] **c3** embed smoke 60-cap 改按类采样
  - `_buildTestBundle` 改为 枚举→能力排除→`_sampleCorpus`(按连续桶 round-robin)→编译。
    desktop(cap=0)逐字节不变。
  - 文件：`scripts/test/xtask_test_embedded.z42`
- [x] **c4** 文档收敛
  - `docs/workflow/testing/README.md`（`test list` + 局部姿势 + 元数据查询）、
    `docs/workflow/testing/platform-tests.md`（smoke 采样）、
    `docs/design/testing/embedded-app-run.md` §5.7（枚举/门控/采样机制原理）。

## 决策记录

- **不让 `test all` 透传过滤收窄**：`test all` 是完整 GREEN 门禁，允许收窄会造成「以为跑了
  全门禁其实只跑子集」的假绿。局部测试入口是 `test e2e --file`/`test stdlib -k`（+c1 最小构建）。
- **`test list` 范围 = 用例级 corpus**（goldens + stdlib `[Test]`）：这是可单独寻址、可过滤的集合。
  cross-zpkg/compiler/platform/runtime-cargo/bench 是 stage 级，归 verify-by-change.md。
- **采样保持 smoke 定位**：不提 cap、不追全量（沿用 2026-08-11 裁决），只让 60 子集更有代表性。

## GREEN

- 完整 `xtask test`（host 6 stages）全绿。
- `xtask test list` 输出正确；`test e2e --file X --no-build` 只重编 X。
- `xtask test embedded --rid iossim-arm64` 采样分布跨类别（本地看 bundle 报告）。
