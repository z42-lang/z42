# Proposal: 补全查询 API（REPL 与 IDE/LSP 共享的语义内核）

> 状态：🟡 SPEC READY（D1–D5 已裁决 2026-07-28）；**排队等 `compiler` 锁** → IMPL 前先跑 D5 spike
> 创建：2026-07-28 | 拟占子系统：`compiler`（查询 API + 透出通道）+ `toolchain`（REPL completer）+ `runtime`（rustyline 补全钩子）

## Why

REPL 的 Tab 补全（`repl-future-tab-completion`）当前 defer，前置写成"0.5.x LSP v1"。经调研，这个耦合画粗了：

**补全的"知识来源"只有一套**——① 当前作用域可见的符号（变量/函数/类型/命名空间成员）② 某表达式/类型的成员列表。这套解析在仓库里唯一实现于 `z42c.semantics`（`SymbolTable`/`TypeEnv`/`MemberResolver`/`Z42Type`）+ `z42c.pipeline`（`DepScan`）+ `z42.core` 反射。IDE 补全与 REPL 补全需要的正是这同一套数据。

因此两者是**「同一个语义内核 + 两个前端」**，不是两回事、也不该做两套：

```
        ┌─────────────────────────────────────────┐
        │  补全查询 API  (本提案的主体，建一次)        │
        │  z42c.semantics: scope 可见符号枚举         │
        │                  类型→可访问成员枚举         │
        │                  ns→导出符号枚举            │
        └───────────────┬─────────────┬─────────────┘
                        │             │
              ┌─────────▼──┐    ┌─────▼──────────────┐
              │ LSP server │    │ REPL completer      │
              │ (LSP 协议)  │    │ (进程内 + live VM)  │
              │  0.5.x 后   │    │  本提案先落地        │
              └────────────┘    └─────────────────────┘
```

关键推论：**REPL completer 不需要 LSP 协议栈**（它进程内已握编译管线 + live VM），把查询 API 抽出来后可**先于完整 LSP 落地**。本提案即"建共享内核 + REPL 作首个客户端"，把 REPL 补全从"等整个 LSP"解耦出来。

## 现状地基（调研实据，file:line）

**已就绪（可直接复用）**
- 编译期成员原料：`Z42ClassType.Fields`/`.Methods`（StrMap name→FieldSymbol/MethodSymbol）+ `OwnFieldNames[]`/`OwnMethodNames[]` + `OverloadsOf(name)`（`z42c.semantics/src/Z42Type.z42:55-134`）；`MethodSymbol` 带 `Visibility`/`IsStatic`/`Signature`/`ContainingTypeName`（`Symbol.z42`）——足够渲染补全项。
- 符号表按名查询完整：`SymbolTable.GetClass/GetFunc/GetInterface/ResolveType`（`SymbolTable.z42:50-72`）。
- 作用域链：`TypeEnv.Vars`/`LocalFns`（public StrMap）+ `LookupVar` 父链（`TypeEnv.z42:12-99`）。
- `StrMap.Keys()`（`z42.ir/src/StrMap.z42:48`）——`Classes`/`Functions`/`Vars`/`Statics`/`Instances` 全 public StrMap，可无侵入枚举。**这是实现"scope/ns 枚举"最短路径。**
- REPL 已缓存依赖世界：`ScriptState.CachedScan`（`DepScanResult`），含 `DependencyIndex`（跨包导出签名 `DepCallEntry.RetType/ParamCount`）+ `Exported[]`（全 TSIG 模块）。

**缺口（本提案要补的主体）**
1. **无面向补全的枚举/查询 public 封装**：`TypeEnv` 只 `LookupVar`（按名，无"flatten 可见名"）；`MemberResolver` 解析方法全 private（无"按类型列成员"）；`SymbolTable`/`DependencyIndex` 无"列 ns 下全部导出"。底层数据都在 public StrMap 上，但都缺封装。
2. **REPL 拿不到语义模型的通道**：`Script.Eval → PackageCompile.Compile` 把 `SemanticModel`/`SymbolTable` 吞进黑盒，只吐 zpkg bytes + 诊断字符串（`Script.z42` `_compileSrc`）。唯一透出的是 `CachedScan`（DepIndex 级，非 SemanticModel 级）。要做补全须在 `CompileArtifacts` 出口新增语义视图透出。
3. **rustyline 补全钩子缺失**：行编辑在 Rust 侧（`corelib/repl.rs` 的 `__repl_readline`）。Tab 补全需 rustyline `Completer` 回调进 VM 取候选——现无此机制。
4. **ISymbol（F2.2）仍缺**：`Symbol.z42` 无共享 ISymbol 基类，`GetSymbol`/`GetDeclaredSymbol`（Roslyn 式统一符号句柄）无法实现。**但补全不强依赖它**（见 D4）。
5. **SemanticModel 无 Expr 级查询**：Phase 1 的 `GetBoundExpression`/`GetExpressionType` 是 C#、未移植到 `.z42`（全仓零命中）。任意 `expr.` 的成员补全需要它做类型推断。

## 提议的形状（Phase 1 = REPL-first，最小可用）

新增一层 **`CompletionQuery`**（`z42c.semantics`，共享内核），三个查询面，全部是现有 StrMap 数据的枚举封装：

```
// 伪签名（DRAFT，最终以 design.md 为准）
public class CompletionQuery {
    // ① scope 可见符号：给活跃 ns + usings + 当前 class + 局部 TypeEnv，枚举可见名
    CompletionItem[] ScopeSymbols(CompletionContext ctx, string prefix);
    // ② 类型成员：给一个已知类型名，枚举可访问 field/method/property（按 visibility 过滤）
    CompletionItem[] TypeMembers(string typeName, bool wantStatic, string prefix);
    // ③ ns 导出：给活跃 ns 集，从 DepScanResult 枚举导出符号
    CompletionItem[] NamespaceExports(DepScanResult scan, string[] activeNs, string prefix);
}
// 轻量补全项（不引 ISymbol，见 D4）
public class CompletionItem { string Name; string Kind; string Detail; string TypeStr; }
```

**REPL completer（首个客户端，`toolchain/scripting` + host）**：
- 从 `ScriptState`（`VarNames`/`Usings`/`DeclNames`/`DeclTypeNames`/`CachedScan`）+ 透出的 SemanticModel 组 `CompletionContext`，调 `CompletionQuery`，前缀过滤，喂给行编辑器。
- `obj.` 成员补全见 D2/D3。

**LSP server（未来客户端，本提案不实现）**：0.5.x 落地时新增 `z42d lsp` 子命令，把文本缓冲+光标映射成 `CompletionContext`，复用同一 `CompletionQuery`，输出 LSP `CompletionItem`。**架构预留，不写代码。**

## 裁决（User 2026-07-28）

- **D1 = A（已定）**：Phase 1 只做 `CompletionQuery` 内核 + REPL completer + rustyline 钩子；LSP / ISymbol / Expr 级类型推断全部预留不做。
- **D2 = 混合（已定）**：静态可推断的类型走**编译期符号**；REPL 定义的活值走**运行时反射**。**安全切分**（关键）：
  - `会话变量.`（`x.` 其中 `x` ∈ `ScriptState.VarNames`）→ **读活值反射**：`x` 已是 VM 里存好的 `Vars{N}` 静态字段，直接读值 + `GetType()` + `Type.GetMembers()`，精确到运行时真实类型，**零副作用**（读字段 ≠ 重 eval 任意表达式）。
  - `类型名.`（静态成员）→ 编译期符号 `CompletionQuery.TypeMembers(name, wantStatic=true, ...)`。
  - 任意 `expr.`（方法链 `foo().` 等）→ 需静态类型推断（缺口⑤）**或**会触发副作用的求值 → **Phase 1 defer 到 Phase 2**。
  - → `CompletionQuery` 与 REPL completer 各承一半：内核提供编译期 `TypeMembers`（类型名/静态）；REPL 侧对会话变量走 live 反射（LSP 客户端将来对应换成静态类型推断，接口一致）。
- **D3 = 接受（推荐默认）**：`PackageCompile.Compile` 出口在 `CompileArtifacts` 加 SemanticModel/CompletionContext 视图字段（REPL 拿语义信息的唯一入口，缺口②）。
- **D4 = 绕过（推荐默认）**：Phase 1 用轻量 `CompletionItem`（name+kind+detail+typeStr）绕过 ISymbol，不碰 F2.2 blocker。
- **D5 = spike 先行**：Tab 补全需 Rust 侧回调进 VM 取候选（rustyline `Completer` ↔ VM callback，或 `__repl_readline` 增补候选提供者回调）。**这是 Phase 1 最大技术未知**——IMPL 第一步先 spike 验 rustyline↔VM 回调可行性与形态，再定实现。

## Scope（Phase 1，A 路径下预估）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.semantics/src/CompletionQuery.z42` | NEW | 三查询面，封装 StrMap 枚举 + visibility 过滤 |
| `src/compiler/z42c.semantics/src/SemanticModel.z42` | MODIFY | 暴露 SymbolTable/scope 供 CompletionQuery（可能加 helper） |
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | MODIFY | `CompileArtifacts` 透出 SemanticModel/CompletionContext（D3） |
| `src/toolchain/scripting/src/*` | MODIFY | REPL completer：组 context + 调 query + 前缀过滤 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | rustyline 补全钩子（D5，先 spike） |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 接线 Tab 补全 |
| `docs/design/toolchain/repl.md` + `docs/design/compiler/*` | MODIFY | 补全机制页 + `repl-future-tab-completion` 前置改为"补全查询 API"；roadmap Deferred Index 更新 |
| 测试 | NEW | CompletionQuery 单测（scope/成员/ns 枚举 + 前缀过滤）；REPL 补全 e2e |

## 子系统 / 并行锁

`compiler`（主）+ `toolchain` + `runtime`。**`compiler` 现被多个 in-flight change 争用**（converge-z42c-onto-z42-project 等排队、unify-run-modes 系列在动）→ IMPL 前按子系统互斥锁排队，DRAFT 阶段不占锁。

## 非目标（Phase 1）

- LSP server / LSP 协议（预留架构，0.5.x）。
- ISymbol（F2.2）/ `GetSymbol`/`GetDeclaredSymbol`（用轻量 CompletionItem 绕过）。
- 任意 `expr.` 的类型推断（SemanticModel Expr 级查询，缺口⑤，独立后续）。
- 签名帮助 / hover / 跳转 / 诊断（LSP 其它能力，各自独立）。
