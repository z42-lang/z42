# Design: 包角色（role）与编译期扩展的解析域

> 背景与动机见 [proposal.md](proposal.md)。本文件是技术设计 SoT：模型 / 解析域 / scripting 判定 / 批次。
> 逐批 scope 见 [tasks.md](tasks.md)。

> ## ⚠️ 2026-09-25 复盘：下面的裁决 ① 与 ④ 已被实测推翻
>
> 本文件其余部分保留原样作为**当时的推理记录**，但这两条不要照着做：
>
> - **裁决 ①（引入 role）→ 取消**。原论证「隔离必须由被保护方声明」有个洞：**物理位置本身
>   就是被保护方的声明，而且是构造式的** —— 不在 `libs/` 里就物理上找不到，不需要任何代码
>   去读字段判断。加「读了再判」的字段是反方向。逐条对照见
>   [tasks.md 批 2.5](tasks.md)。
> - **裁决 ④（scripting 拆两包）→ 取消**。「eval 内核零编译器域依赖」的前提不成立：
>   `Script.Eval` 直接调 `Classifier` / `Rewriter`，两者都用 `Z42.Syntax` 的 Lexer。拆完
>   内核仍依赖 `z42c.syntax`，达不到目的。
>
> **仍然成立的是 §scripting 的那句核心判断**：「这不是抽象问题，是**分发**问题」。它的解法
> 落在 [add-deployment-model](../add-deployment-model/)：`ModuleSearch.Dirs()`（#832）、
> `probing-paths`（#816）、zpkg 产物引用（#836）。

## User 裁决（2026-09-24 会话）

proposal 的三个待裁决点已裁，另补一条 proposal 列为 Out of Scope 的 scripting 判定：

| # | 裁决 | 依据 |
|---|------|------|
| ① 要不要引入 role | **要**。批 1–3 全做，批 4（`z42c.abi` 契约包）推迟 | 症状（用户写不了 generator）根因确为「一个物理位置决定三件事」 |
| ② `compile-time` / `contract` 合一？ | **合一**，只留两值 `runtime`（默认）/ `compile-time` | proposal 自陈「目前看不出区别」；快速开发期不预造区分，真出现差异再拆 |
| ③ 解析域目录名 | **`compiler-libs/`**，与 `libs/` 平级 | 名字自解释，不嵌套进 `libs/`（嵌套会让「普通工程只看 libs/」这条规则变成前缀判断） |
| ④ **scripting 归属**（proposal 的 Out of Scope） | 拆两包 + 发行形态分离，见下 §scripting | 本次会话实测，见下 |
| ⑤ `z42c.semantics` 公开面稳定性 | **unstable，不给兼容承诺** | 快速开发期。对冲 = handler ABI 握手 fail-fast 提前到批 2 |

> ⑤ 的对冲不可省：越不承诺稳定，越需要 fail-fast。`GeneratorLoader` 今天靠「接口同一性」工作
> （[GeneratorLoader.z42:16](../../../../src/compiler/z42c.pipeline/src/GeneratorLoader.z42#L16)），
> handler zpkg 若由不同代编译器编出，失败模式是**崩**而不是报错。

## Model：role 与 kind 正交

`[project].role`，与既有 `kind`（lib/exe）正交：

| role | 含义 | 能被谁引用 | 链入产物 | 落在哪 |
|---|---|---|---|---|
| `runtime`（默认，省略即此） | 普通库，运行期在场 | 任何工程的 `[dependencies]` | 是 | `libs/` |
| `compile-time` | 只在编译器进程里活（契约 + 插件本体） | `[analyzers]`；以及 `role = compile-time` 工程的 `[dependencies]` | **永不** | `compiler-libs/` |

三条今天靠代理判据的规则改为直接读 role：

| 规则 | 今天的代理判据 | 改为 |
|---|---|---|
| 进不进 SDK `libs/` | 是不是 stdlib workspace 的 `default-members` | role |
| publisher 要不要 bundle 进 apphost payload | **在不在 shipped `libs/`** | role |
| 递归穿透时算不算「框架、到此为止」 | **在不在 `src/libraries/` 目录下** | role |

`builder_publish.z42:546-551` 那两条打补丁的注释（`z42.scripting`「越界」、`z42.workload.*`「漏拷」）
随之退休——两者都不是 bug，是角色表达不出来。

## scripting 判定（proposal 的 Out of Scope，本次补齐）

### 实测事实（2026-09-24）

**一、抽象早已做完，缺的不是抽象。**
[ReplCompilerHost.z42](../../../../src/libraries/z42.scripting/src/ReplCompilerHost.z42) 里编译能力已是
门面 + 运行期反射注入（`IReplCompiler` ← `ModuleLoader.Load` ← `Z42cReplCompiler`）。它的头注自陈：

> 组件缺失（runtime-only SDK）→ `NoReplCompiler` 兜底（**编译恒失败、补全恒空**）

而 `_findCompilerZpkg` 的四条探测路径（`Z42_HOME/programs/z42c/` / `Z42_PORTABLE_VM` 反推 SDK 根 /
开发树 artifacts / `Z42_LIBS`）**全部指向 SDK 布局，纯 runtime 包一条都命不中**。

⇒ **今天 runtime 包里的 `z42.scripting` 已经是恒失败的空壳**。"runtime 支持 scripting" 是名义上的。
这不是本设计造成的，是现状；本设计只是让它显形。

⇒ 结论：**这不是抽象问题，是分发问题。** eval 的实现就是编译器，抽象只能把「编译期依赖」变成
「运行期缺件」，变不出编译器。

**二、四条编译器域依赖里，一条是纯假的。**

| 依赖 | 实际用量 | 判定 |
|---|---|---|
| `z42.ir` | **只**为 `Script.FormatVersion()` 拼一句版本串（`ZbcVersion` + `ZpkgWriterZ` 两个常量）| 🔴 假依赖 → **批 0 已断** |
| `z42c.syntax`（+ `z42c.core` 的 `Span`）| Lexer/Token —— Classifier / Completeness / Completer / Rewriter | ✅ 真依赖 |
| `z42.build` | `IReplCompiler` 门面 | ⚠️ 门面住在编译器域包里，意义被落点抵消 |

### 设计

**① 拆两包**，拆分线天然存在（也正好是使用场景线）：

| 包 | 文件 | 编译器域依赖 | role |
|---|---|---|---|
| `z42.scripting`（eval 内核）| Engine / Script / ScriptState / EvalResult / ReplCompilerHost / Playground | **零** | `runtime` |
| `z42.scripting.editing`（编辑器辅助）| Classifier / Completeness / Completer / Rewriter | `z42c.syntax` | `compile-time` |

`Completeness`（判断输入完整、要不要续读）是 **tty REPL** 的需求；嵌入宿主传整段源码求值，四件全不用。

**② `IReplCompiler` 门面搬家**：从 `z42.build`（编译器域）挪到 `z42.scripting` 自己。一个「为了不依赖
编译器」而设的门面，自己住在编译器包里，落点抵消了意义。

**③ 发行形态**：runtime 包回归纯执行；要 eval 的嵌入宿主取 **`z42-runtime-scripting`**（= native + std
libs + z42c 组件）。对标 .NET：`Microsoft.CodeAnalysis.CSharp.Scripting` 是可选包，运行时本身不带 Roslyn。

> ③ 的替代方案是「runtime 包干脆不带 scripting，要 eval 就用 SDK」——更省事，但嵌入场景要扛整个 SDK
> 体积。**倾向新增包，批 1 实施前最终确认。**

## 批次

| 批 | 内容 | 状态 |
|---|---|---|
| **0** | 断 scripting 的假依赖（`z42.ir`）：`FormatVersion` 迁 z42i | ✅ 见 tasks |
| **1** | role 落地：`compiler-libs/` 解析域 + publisher 读 role + scripting 拆两包 + 门面搬家 | ⬜ |
| **2** | `kind="analyzer"` + `[analyzers]` 支持 `path` + 隔离校验 + **handler ABI 握手 fail-fast** | ⬜ |
| **3** | 大重命名：`std.*` 用户库 + `z42c.*` 编译器域（含 ir/project/build） | ⬜ |
| **4** | `z42c.abi` 契约包 | ⏸️ 推迟（裁决 ⑤：形态稳定后再谈） |

### 批 0 注：FormatVersion 的终局

批 0 把 `.version` 迁 z42i 是**职责归位**（`.version` 是 REPL 前端的展示职责，不是 eval 内核的能力），
不是绕路。但终局不是这里：**这两个数该由 VM 自报**——VM 才是「能加载什么格式」的权威，Rust 侧
`ZPKG_VERSION_MAJOR/MINOR` 已在（`src/runtime/src/metadata/zbc_reader/versions.rs`），只是未经 builtin
暴露给 z42。`Script.z42` 原注释也记着这个 follow-up（`repl-future-runtime-version`）。

届时 `_formatVersion` 改调 `Std.Runtime`，**z42i 也不再需要 `z42.ir`**。需新 builtin ⇒ 走 vm 类型完整
变更流程，不搭批 0 的车。

## 参照系

见 [proposal.md §参照系](proposal.md)（Rust / Swift / Java / .NET / KSP / Dart / Scala 七家）。三条结论：

1. z42 今天踩的是**有名字的已知失败模式**——「契约 API 在 SDK 目录里、但不在解析器会去看的地方」，
   插件作者根本写不出插件，且没有任何东西会诊断它。
2. 「独立 kind」比「独立 scope」强，但两个都要。**z42 已有 scope（`[analyzers]` 就是 processorpath），
   缺的正是 kind** —— 批 2 补它。
3. 契约由工具链发（Java/Rust 路线）是最高杠杆的选择，**但必须落在解析器看得见的地方** —— 批 1 做这个。
