# Proposal: z42c.ir + zpkg 元数据后端收敛入 stdlib（z42.ir / z42.metadata）

> 状态：**DRAFT（待 User 确认后实施）** | 创建：2026-07-21
> 子系统：**compiler**（z42c.ir / z42c.project 删 + 5 包调用点改引用）+ **stdlib**（新增 z42.ir / z42.metadata member）
> **不持锁**；开工前按 parallel-development 登记 compiler + stdlib 双锁。
> 沿用 [`converge-z42c-onto-z42-project`](../converge-z42c-onto-z42-project/proposal.md) 收敛范式（roadmap.md:122 列项）。

## Why（为 REPL 共享编译栈基础库）

REPL（0.4.x capstone）要**交互式编译片段 → IR → zbc → 跑**，并**读 zpkg 元数据**做增量/符号解析。
这套「IR 模型 + zbc·zpkg 二进制格式 + 类型导出元数据」现全锁在编译器私有包
（`z42c.ir` / `z42c.project`）里，REPL / 其它工具要用只能整体依赖编译器。

把它们下沉到 stdlib（`z42.ir` / `z42.metadata`），则 REPL、z42c、z42b、未来的 Playground /
分析工具**共享同一份实现**——与 [`converge-z42c-onto-z42-project`](../converge-z42c-onto-z42-project/proposal.md)
把清单模型下沉到 `z42.project` 同一范式、同一动机（消除「编译器私有 vs 共享」的重复面）。

**现状已铺好一半**：converge-z42c-onto-z42-project 阶段 1/2 已把 `z42c.project` 的**清单模型**
（ProjectModel/ManifestLoader/SourceDiscovery/PathTemplate）删除、下沉到 `z42.project`（stdlib，
byte-identical 7/7 已验证）。`z42c.project` 如今**只剩 zpkg 后端** 7 文件。本 change 接着走完：
IR + zpkg 后端也下沉。

## 规范张力（须先裁决）

**converge-z42c-onto-z42-project 的 design 决策 1 原判**：zpkg 后端是「编译器产物机器，z42.project
按设计永不含」，故计划把它**留在编译器**、独立成 `z42c.zpkg`（compiler-local）。

**本 change 与之相反**：REPL 需要读/写 zpkg 元数据 → 后端应**入 stdlib**（`z42.metadata`）。

> **裁决建议（User 已选「z42.ir + zpkg 元数据后端」范围，即认可后端入 stdlib）**：以本 change 为准——
> zpkg 后端下沉 `z42.metadata`，`z42c.zpkg` 计划作废（该阶段 2 rename 本就未落地，无沉没成本）。
> 理由：后端是**格式**（zbc/zpkg 读写），格式是跨工具契约、天生可共享；「编译器产物机器」是**用法**
> 不是**归属**。同步更新 converge-z42c-onto-z42-project/design.md 决策 1 为「后端下沉 z42.metadata，
> 不再独立 z42c.zpkg」，保持唯一真相来源。**（此张力已按 CLAUDE.md 规范冲突检测流程摆出，待 User 拍板。）**

## What Changes

### 新增**一个** stdlib 库 `z42.ir`（User 定：两半合并为一）

| 库 | 内容（从哪来）| namespace | deps |
|----|--------------|-----------|------|
| **z42.ir** | ① z42c.ir 全部：IR 模型（IrType/IrInstr/IrModule/IrTerminator/TypedReg/ObjectMethods）+ zbc BinaryFormat（ByteWriter/ZbcFormat/ZbcInstr/ZbcReader/ZbcReaderInstr/ZbcStringPool/ZbcWriter/TokenAllocator）+ ExportedTypes + DependencyIndex + StrMap；② z42c.project zpkg 后端：ZpkgReader/ZpkgWriter/ZpkgWriterIndexed/ZpkgBuilder/PackageTypes/TsigReconcile | `Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Project`（三者**均不改**，MOVE 无并存 → 调用点零改）| z42.core（prelude）+ z42.encoding + z42.io + z42.crypto |

合并为一：IR 与 zpkg 后端本就单向耦合（zpkg→ir），REPL 两半都要；一个库 = z42c 只加一条 dep、
一次拓扑，最简。namespace 保持三段不变（库可含多 namespace），调用点 `using Z42.IR;` / `using
Z42.Project;` 一字不改。

### 子决策（User 定）

- **CacheStore**（增量构建缓存）→ **留构建侧**：不入 z42.ir。消费者是 z42c.driver + z42c.pipeline
  （增量构建），随删 z42c.project 时**迁入 z42c.pipeline**（保持 namespace `Z42.Project`，消费者零改）。
- **StrMap**：z42c 自带 map util，随 z42.ir 平移；与 stdlib 容器去重列后续。

### 编译器侧改动

- **删** `src/compiler/z42c.ir/`、`src/compiler/z42c.project/`（内容下沉 z42.ir；CacheStore 迁 z42c.pipeline）。
- z42c.semantics / z42c.pipeline / z42c.driver 的 deps：`z42c.ir` + `z42c.project` → 单一 `z42.ir`。调用点因 namespace 不变而**零改**（仅 toml deps 换名）。
- 拓扑：z42.ir → z42c.*（stdlib 先建）。
- workspace toml（compiler + libraries）member 表 + CI 拓扑同步。

## Scope（允许改动的文件，详单见 tasks.md）

`src/libraries/z42.ir/**`（NEW）· `src/libraries/z42.metadata/**`（NEW）· `src/libraries/z42.workspace.toml`（member+topo）· `src/compiler/z42c.ir/`（DELETE）· `src/compiler/z42c.project/`（DELETE）· `src/compiler/z42.workspace.toml`（member）· z42c.{semantics,pipeline,driver} 的 `.z42.toml`（deps 换名）· CI ci.yml（拓扑）· 文档（compiler-architecture / project / doc-system 索引）。

## 非目标（Out of Scope）

- IrGen（codegen 逻辑）不动——留 z42c.semantics（它 emit IR，用 z42.ir 的模型）。
- REPL 本身（`add-z42-repl`）独立 change；本 change 只把它要共享的基础库就位。
- StrMap 与 stdlib 容器去重、zbc/zpkg 格式演进——各自独立。
- runtime（Rust）metadata 不动：那是 VM 侧 zbc/zpkg **读取器**（Rust），与 z42 侧 writer 是两个实现，不在本收敛内。
