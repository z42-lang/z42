# Proposal: Attribute 与编译期 Handler 体系

> 状态：DRAFT（决策已定，见 [design.md §决策记录](design.md)）。多 PR 阶梯，部分带格式 bump。

## Why

z42 的 attribute 机制**已做对一半、烂一半**：

- ✅ **用户 attribute** 走干净路径：`[Foo]` → `AttributeSynth` 合成工厂 → `IrAttrRef` → zpkg → 运行时反射
  重建。四层级 `GetCustomAttributes` 全通。
- ❌ **内建 attribute** 是乱源：一张硬编码名字白名单（`Native/Test/Benchmark/Skip/ShouldThrow/Timeout` +
  `Setup/Teardown/Ignore`），每个走各自 ad-hoc pass（`StubEmitter`/`TestIndexBuilder`/`BenchmarkDesugar`）。
  **按名字魔法识别**——开放不了、也解释不清。

C# 把乱推到极致：一种 `[X]` 语法掩盖三个正交维度（谁消费 / 何时生效 / 是否活到运行时），外加
pseudo-attribute（`[StructLayout]` 是元数据位、`[Route]` 是 blob，同语法两套存储）。

## What Changes

用 **"枚举编译器扩展点、不枚举 attribute"** 的模型收敛。三消费模型（litmus 判据见 design.md §3）：

1. **store-meta**（惰性元数据，运行时反射）：`[X]` 单一语义，≈ 现状路径，用户自由扩展。
2. **built-in directive**（编译期、封闭、codegen 原生）：`layout`/`extern`/`repr`，IrGen 直接读 descriptor 位。
3. **open handler**（编译期、可注册）：**Generator**（源生成，≈ C# source generator，但支持 Replace/Augment、
   免 partial）+ **Analyzer**（诊断/lint，四级 severity + `[lints]` 可调级）。同一 handler 注册表，
   同 VM / 同 zpkg 加载 / 同反射。

**声明位一律 `[X]`，不引入任何新关键字**（D1/D7）：attribute 类型沿用 `class Foo : Attribute`，位置限制用
`[Targets(...)]`、richer 约束靠 handler 自校验（Rust 对齐）；`layout`/`extern`/`deprecated` 等也是 `[X]`
directive，不做关键字。`caller_member!()` 等**表达式位编译期宏**吸收"编译器插入值"整类。

`[X]` 的 kind 完全由 **"X 解析到什么类型"** 三路判定（directive 注册表 → 实现 handler 接口 → else
store-meta），**既无魔法名字表、也无 marker**；这替换掉现有 `_isUserAttr` 白名单。现有 4 个 ad-hoc pass
全部收敛进统一契约（映射见 design.md）。

## 前置依赖

- 反射 MVP C1–C3（`GetCustomAttributes` / typeof / Attribute）——**已完成**，是 Test 家族降级 store-meta 的地基。
- 无其它阻塞。

## Scope（允许改动的子系统 / 文件）

- `compiler`：`src/compiler/z42c.*`（pipeline / semantics / IrGen / 现有 4 pass / 新增 HandlerRegistry）。
- `stdlib`：`src/libraries/z42.core`（`Std.Meta`：Analyzer/Generator/ModuleGenerator 接口、DiagRule、
  `Target` 枚举、directive 类型；`Std.Attribute` 基类保留）。
- `runtime`：`src/runtime`（metadata reader：directive 烘焙结果 / deprecated / caller-kind flag 的读取；随对应带 bump PR）。
- `toolchain`：`packages.toml` 解析（`[lints]` 段、generator/analyzer 依赖）。
- `docs`：book 对应机制页 + 本 change 目录。

> 逐 PR 的精确文件 scope 见 [tasks.md](tasks.md)。**PR1 严格限于内部重构、零新语法、零 bump。**

## Out of Scope（本 change 不做，Deferred）

- 用户可写的 `macro` / 用户自定义 derive（保持编译器/团队掌握内建集；架构预留将来开放）。
- 编译期 handler 沙箱（v1 直接信任 + 确定性约束，见 D5）。
- `[Layout]`/`[Repr]`（E2，随 interop 需求）；`OnIrOp` IR 层性能 lint（additive，随 perf 需求）。
- Rust `[track_caller]` 多层传播（留作宏注册表上的将来追加，见 D3）。
- `[Native]`→`[Extern]` 改名（User 裁决 2026-08-19 暂不做，独立小 change）；局部变量 attribute（需扩 parser）。

## Open Questions

无未决项——D1–D8 + 两个缺口均已定案，见 [design.md §决策记录](design.md)。其中 **D8 类名后缀约定反转了
z42 旧"无后缀"决定**（已按 CLAUDE.md 规范冲突流程经 User 裁决），实现 PR 须同步 `Attribute.z42`/`basic.z42`
头注。实施中若再遇规范冲突，照旧停下裁决。
