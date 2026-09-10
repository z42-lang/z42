# Tasks: 零发现测试的测试单元判红

> 状态：🟢 已完成 | 完成：2026-09-11
> 类型：`fix`（最小化模式）。

**变更说明：** `Std.Test.Runner.RunModule` 在被要求跑一个**一条 `[Test]`/`[Benchmark]` 都没发现**的
产物时，报错并返回非零，而不是打印 `0 passed, 0 failed, 0 skipped` 然后 **exit 0**。
**原因：** 见下「实测」。
**文档影响：** `src/libraries/z42.test/README.md`（若有行为段）、`docs/design/testing/testing.md`。

## 实测（改之前跑过的）

构造一个只有一个普通函数、连一条 `[Test]` 都没有的测试包，交给 z42b：

```
  Result: 0 passed, 0 failed, 0 skipped
▶ exit code = 0
```

**静默通过。** 于是今天下列任何一种情况都是绿的：

- attribute 拼错（`[Tests]` / `[test]`）→ 不进 TIDX → 该文件的测试**全部消失**，绿；
- `[sources]` glob 写错 / 文件漏进包 → 同上；
- 测试函数被改名或误删 → 同上；
- `_dirHasTestMethods` 的**子串扫描**（`File.ReadAllText(f).Contains("[Test]")`）把一个只在注释里
  提到 `[Test]` 的目录当成测试单元 → 编出来 0 个测试 → 绿。

即：**「我的测试没跑」与「我的测试全过」在退出码上不可区分。**

## 为什么现在做

1. 它本身是个真洞（上面四条都能发生）。
2. 它是 [`systematize-test-pipeline`](../systematize-test-pipeline/design.md) **S2 的必需安全网**：
   S2 要让 `[Test]` 默认不进 zpkg、只有测试构建才保留。xtask 里有 **31 处 `z42c build` 调用点**
   （18 个文件），漏传一处，那条路径上的测试就会**静默消失**而不是报错 —— 正是本条要堵的失败模式。
   先有这道门，S2 才敢做。

## 非破坏性（实测）

全 GREEN gate（13 stage，含 `stdlib [Test]` / `stdlib [Benchmark]` / `compiler`）里
**当前零个单元命中** `Result: 0 passed, 0 failed, 0 skipped`。

## 任务

- [x] 1.1 `Runner.RunModule`：`runnable == 0` → 打印明确错误（含产物路径 + 可能原因）并返回非零
- [x] 1.2 json 格式下仍输出合法报告（不破坏 `bench --json` 的聚合消费方），错误走 stderr
- [x] 1.3 `RunModuleResults`（bundle 路径）**本次不动** —— 聚合语义不同（多模块里某个空是否算错另议），
      记为跟进项
- [x] 1.4 测试：退出码决策抽成纯函数 `Runner.ExitCodeFor(runnable, failed)`，dogfood 三例直测
      （0 runnable → 1 / 有失败 → 1 / 全过 → 0）；端到端另有实测见下
- [x] 1.5 GREEN：`xtask test` 全 13 stage


## 端到端实测

改前（零测试的探针包）：
```
  Result: 0 passed, 0 failed, 0 skipped
▶ exit code = 0          ← 静默通过
```
改后（同一个包）：
```
  Result: 0 passed, 0 failed, 0 skipped
error: no [Test] or [Benchmark] found in `.../zt.probe.zpkg`
  a test artifact with zero discovered tests is treated as a failure —
  check the attribute spelling, the package's [sources] globs, and that
  the test functions still exist in the compiled artifact.
▶ exit code = 1
```

报告照常打印（`bench --json` 的聚合消费方不受影响），错误另走 stderr。

## 验证

- `xtask test`：**全 13 stage 绿**。
- 非破坏性实证：全 gate 内**零个**单元命中 `Result: 0 passed, 0 failed, 0 skipped`（改前量过）。
- dogfood 新增 3 例覆盖 `ExitCodeFor` 三个分支。

## 过程中的一条教训（已知坑）

`xtask` 二进制把 `scripts/` 的内容**编译进去**（gate stage 清单、vscode 关键字分类表等）。
**拉完 main 必须先重建 `xtask` 再信 gate**，否则会看到假故障：本次先后误判了两次 ——
① gate stage 清单「代码 10 vs 文档 13」；② `vscode-syntax` 报 `methodof` 未分类（其实
`_kwOperatorExpr` 里早有）。重建命令：

```bash
rm -rf artifacts/xtask/.cache artifacts/xtask/xtask.zpkg
.z42/z42 publish scripts/xtask.z42.toml
```

（注意：不清 `.cache` + `.zpkg` 时 `publish` 只会把旧 zpkg 重新包一个 apphost，**不重编**。）
