# Proposal: z42b 接管测试目标 —— 从真 manifest 建/跑 `[[test]]`，不再伪造 manifest

## Why

### 这是已规划的下一段，不是新想法

[cross-platform-testing.md:24-29](../../../design/testing/cross-platform-testing.md) 记着本程序的阶段：

> ②a **host compile-then-test 首刀（已落，`add-z42b-compile-then-test`）**：`z42b test <z42.toml>`
> ③ z42b in-process 编译成熟后，「编译一个项目」也走 z42b（语料级编译仍留 xtask）

**已有**：`z42b test <project.z42.toml>` → 进程内编译器建项目 → 反射跑产物
（[builder_test.z42:65-72](../../../../src/toolchain/builder/core/builder_test.z42#L65)）。
**缺的**：它建的是**项目自己**，不认 `[[test]]` 目标。本变更补这一段。

### 症状：xtask 伪造 manifest，把信息丢了

z42c 只会「编译一个 manifest 描述的包」，不会「编译某个包里的某个目标」。于是 xtask **伪造一个
mini-manifest**，让测试目标看起来像一个包：

```toml
[project]                              # ← 全部由 xtask 拼出来，用户看不见
name    = "z42.collections.test.queue_tests"
kind    = "lib"
[sources] include = ["tests/queue_*.z42"]
[dependencies] ...                     # [dependencies] ∪ [tests.dependencies] ∪ [[test]].dependencies
[build] output_dir = ... cache_dir = ...
```

里面**没有一项是新信息**，全是把已有配置换个格式。但这一搬丢掉了「我是 `z42.collections` 的测试目标」
这条身份，直接导致两个问题：

| 问题 | 后果 |
|---|---|
| **编译器分辨不出测试包与普通包** | 两者都是 `kind = "lib"` 的普通 manifest ⇒ 在 `src/` 里写 `[Test]` 编译器照单全收：写进库 zpkg、附 TIDX section、把 z42.test 拖进发布依赖，**零警告** |
| **测试测不了 `internal`** | 测试成了父库的**跨包消费者**，而 `internal` 是包作用域、跨包强制 ⇒ 只能测公开面 |

> **否决过的两条路**：
> ① 「测试搬进 `src/` + 条件编译」（Rust 路线）—— User 指出真实代价：库源码要**编译两遍**。
>    实测确认现在每个文件只编一次（合成 manifest 的 `[sources]` 只含测试目录，父库是已建好的依赖）。
>    **不引入双编译**是硬约束。
> ② 「给合成 manifest 加一个 `dev-of` 字段把身份传下去」—— 是**为了保住一个不必要的降级**而加字段，
>    降级本身才是病根。User 裁决：直接做正解。

## What Changes

**z42b 从真 manifest 解析测试/bench 目标，自己编、自己跑。xtask 不再伪造 manifest。**

```bash
z42b test                          # 默认 z42.toml：编译 + 运行**全部** test 目标
z42b test <project.z42.toml>       # 指定 manifest
z42b test --name queue_tests       # 只编译 + 运行这一个 [[test]]
z42b bench / z42b bench --name ...  # bench 同构（User：bench 与 test 同等处理）
```

配置面同时收敛（**用户要写的东西变少**）：

```toml
[tests]                                  # 配了 glob → 全部编译 + 全部运行
include = ["tests/**/*.z42"]
[tests.dependencies]
"z42.test" = "0.1.0"

[[test]]                                 # 要点名 / 要自驱时才写
name    = "exit_ok"
harness = false                          # 自己写 Main，退出码判定
include = ["tests/exit_ok.z42"]
# entry 不写 —— 自动探测（ZpkgBuilder.AutoDetectEntry 四级优先：.Main / Main / .main / main）
```

**一个目标最少只要 `name` + `sources`**；`harness` 默认 `true`，`entry` 默认自动探测。

因为是 z42b **自己**从真 manifest 选的「建 X 的 test 目标」，**父包是谁它当然知道** ⇒
交给进程内编译器即可：
1. **`internal` 可见**：加载父包符号时不设 `IsImported`；
2. **`[Test]` 只能出现在测试目标里**：其余包出现即报错。

**不需要任何新 manifest 字段。**

## Scope（允许改动的文件）

**刀一（本变更）——单平台 host 跑通**

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/builder/core/builder_test.z42` | MODIFY | 目标解析（`[[test]]` + `[tests]` glob）+ `--name` + 遍历建/跑 |
| `src/toolchain/builder/core/builder_build.z42`（或 `_buildProject` 所在文件） | MODIFY | 加「按给定源集 + 依赖集 + 输出路径建目标」的路径；packed 强制 |
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | MODIFY | `CompileInputs` 加「父包名」（internal 放行用） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 被加载包 == 父包 → 不设 `IsImported` |
| `src/compiler/z42c.semantics/src/DeclEnforcer.z42` | MODIFY | 非测试目标里出现 `[Test]`/`[Benchmark]` → 新码 |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新码 E0457 |
| `src/libraries/z42.project/src/RunTarget.z42` | MODIFY | `entry` 变可选（`harness=false` 不再必填） |
| `scripts/test/xtask_test_lib_units.z42` | MODIFY | 合成 manifest 退休 → 转发 z42b |
| `src/toolchain/builder/tests/**` | NEW | z42b 目标解析 / `--name` 选择的单测 |
| `src/tests/cross-zpkg/dev_target_internal/**` | NEW | internal 可见性 golden（**普通消费包仍被拒**） |
| `docs/**` | MODIFY | 见文档同步 |

**刀二（独立）**：并行策略、`harness=false` 的 exe 路径收编、其余 xtask 测试路径
（`targets` / `dist` / `cross`）转发、`[tests]` 段缺失 → 不发现（破坏性，含补 28 个 manifest）。

## Out of Scope

- **双编译路线**（测试进 `src/` + cfg）——已否决。
- **`dev-of` 字段**——本方案使其不必要。
- **语料级编译**（`src/tests/**` 的 golden）仍留 xtask（阶段 ③ 原文即如此）。
- **`private` 跨包可见**——不给；边界同 C# `InternalsVisibleTo`。

## Open Questions

- [ ] `--name` vs 位置参：位置参与「位置参是 manifest 路径」冲突，故取 `--name`（User 未否）。
- [ ] `[tests] include` 的 glob **一文件一单元** vs **整段一单元**：本刀**保持今天的分法**，不改。
