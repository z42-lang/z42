# Tasks: 发布源码里不得出现测试 attribute（gate stage）

> 状态：🟢 已完成 | 完成：2026-09-11
> 类型：`test`（新 gate stage，最小化模式）。
> 上游：[`systematize-test-pipeline`](../systematize-test-pipeline/design.md) 的 **P1**
> （「测试不进产物」今天只靠目录纪律、无机制）。

**变更说明：** 新增 `shipped-src` gate stage —— 扫 `src/libraries/*/src/**` 与
`src/compiler/*/src/**`（会被编进发布 zpkg 的源），行首出现 8 个内建 test attribute 任一个即红。
**原因：** 见下。
**文档影响：** `docs/book/src/dev/test-gate.md` 的 gate-stages 区 + 说明段。

## 为什么

在 `src/libraries/<lib>/src/` 里写一个 `[Test]`，编译器**照单全收**——写进该库的 zpkg、
附一个 TIDX section、把 z42.test 拖进发布依赖——且**零警告**。
「测试代码不进发布产物」这条约定今天**只靠纪律**维持。审计过：当前**一处都没有**，
所以这是在纪律还在的时候把它变成机制（同 `walkers` stage 的理由：*没有测试盯着的约定迟早会烂*）。

## 为什么不在编译器里判（原计划）

原打算做成编译期诊断（「非测试包里的 `[Test]` 判错」），实施前发现**做不到**：
**编译器分辨不出自己在构建「测试包」还是「普通包」**——两者都是 `kind = "lib"` 的普通 manifest
（测试单元走 xtask 合成的 mini-manifest，[`_renderSyntheticManifest`](../../../../scripts/test/xtask_test_lib_units.z42)，
形状与库 manifest 完全一样）。

要在编译器层面判，得先引入一个**新的项目模型信号**（「谁是谁的测试包」）。而那个信号恰好也是
**友元访问**（让测试包看得见被测包的 `internal`）所需要的，故留到那件事里一起设计，不在这里
单独造一个。

在此之前，本门守住**真正有害的那一段**：会被发布出去的源码。代价是覆盖面只到本仓
（不像编译期诊断能保护所有 z42 项目）——这是明确的取舍。

## 任务

- [x] 1.1 `scripts/test/xtask_test_shipped_src.z42`：扫描 + 检测 + 报错（纯文本，~0.9s）
- [x] 1.2 `xtask_test.z42`：注册进 `_gateStageNames()` + 挂载执行（stage 9，可 `--skip shipped-src`）
- [x] 1.3 `xtask_cli_test.z42`：加独立子命令 `xtask test shipped-src` + help 登记
      （与 `lines` / `walkers` 对齐，便于单跑调试）
- [x] 1.4 `docs/book/src/dev/test-gate.md`：gate-stages 区加一行 + 说明段
      （**这两处必须同改** —— gate 自带清单/文档对账门）
- [x] 1.5 验证

## 验证

- **正向**：`xtask test shipped-src` → `✓ no test attributes in 510 shipped source file(s)`。
- **负向**（真判红）：临时往 `z42.collections/src/Queue.z42` 末尾加一个 `[Test]` 函数 →
  ```
  ✗ src/libraries/z42.collections/src/Queue.z42:92: [Test]
  error: 1 test attribute(s) found in shipped package sources
  ▶ exit = 1
  ```
  探针已还原（`git diff src/` 为空）。
- `xtask test`：**全 14 stage 绿**（新 stage 0.9s）。

## 实施记录

- 失败路径**不调 `_procEnd`** —— 否则会在报错后打出一个 ✔（第一版犯了，对齐 `_testLines` 的惯例）。
- 注释行（`//` 前缀、块注释续行 `*` 前缀）必须跳过：编译器源码里有**大量**讲解 `[Test]` 的注释
  （`TestIndexBuilder` / `HandlerRegistry` / `BenchmarkDesugar` 抬头都是）。
- 8 个 attribute 名在 xtask 里是**独立的一份文本级清单**（xtask 不依赖 z42c.semantics）——
  加新内建 test attribute 时两边都要动，脚本头注已写明这份冗余是故意的。
