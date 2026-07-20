# z42 编译器内部实现原理

> **目的**：记录 C# bootstrap 编译器的内部数据结构、算法、加载策略与关键设计决策。
> 让新接手者不必阅读大量源码即可理解"为什么这样设计"。
> 面向语言设计师和编译器开发者，不面向 z42 语言使用者。
>
> 使用者视角请看 `docs/design/language/language-overview.md`、`docs/design/compiler/compilation.md`。

---

## Pipeline 顶层流程

```
source.z42
  │
  ├── Lexer (TokenDefs + LexCombinators)          [z42.Syntax/Lexer]
  │      → Tokens
  │
  ├── Parser (Pratt + 组合子)                     [z42.Syntax/Parser]
  │      → CompilationUnit (AST, sealed record)
  │
  ├── BenchmarkDesugar (AST→AST, pre-typecheck)   [z42.Semantics/Codegen]
  │      → CompilationUnit (Bencher-arg [Benchmark] rewritten)
  │
  ├── TypeChecker (SymbolCollector + SymbolTable) [z42.Semantics/TypeCheck]
  │      → SemanticModel
  │      ↑ 读：ImportedSymbolLoader from TsigCache
  │
  ├── IrGen (FunctionEmitter + Ctx)               [z42.Semantics/Codegen]
  │      → IrModule (registers SSA-ish)
  │
  └── ZbcWriter / ZpkgWriter                       [z42.IR/BinaryFormat]
         → source.zbc / package.zpkg
```

单文件模式走 `SingleFileCompiler`；项目模式（`.z42.toml`）走 `PackageCompiler`，
两者共享 `PipelineCore` 的 TypeCheck + Codegen 阶段。

> **BenchmarkDesugar**（add-benchmark-bencher-arg-trampoline, 2026-05-31）：
> 纯 AST→AST 转换，在 `PipelineCore.CheckAndGenerate` / `CheckOnly` 内、
> TypeChecker 之前运行。把 `[Benchmark] void f(Bencher b)` 重写为零参 wrapper
> `[Benchmark] void f() { var b = new Bencher(); f$impl(b); b.printSummary("f"); }`
> + 降级 helper `void f$impl(Bencher b)`（剥 attribute）。合成代码走正常管线
> 解析/类型检查/codegen，故 validator/runtime/runner 零改动。放在 pre-typecheck
> 是关键：validator 永远只见已支持的零参形态。详见
> `docs/design/testing/testing.md` Benchmark 章节。这是目前唯一的 AST-level
> desugar pass；未来若有更多 lowering（如 `throw;` rethrow desugar 已在
> TypeChecker 内联做）可考虑提取统一的 desugar 阶段。

Workspace 模式（`z42.workspace.toml`）走 `ManifestLoader` 发现 + 共享继承（C1）→
后续 `WorkspaceBuildOrchestrator` 拓扑编译（C4）。

---

## Parameter Modifiers（`ref` / `out` / `in`，2026-05-05）

**编译期数据流**（运行时 codegen / VM 部分留给 follow-up spec `impl-ref-out-in-runtime`）：

```
源码:   void Foo(ref int x)           // signature 修饰符
        Foo(ref c)                    // callsite 修饰符 + `out var x` 内联声明

Lexer:  KW_REF / KW_OUT tokens
        (KW_IN 复用 foreach `in` token)
        ↓
AST:    Param { Name, Type, Default, Span, Modifier: ParamModifier (None/Ref/Out/In) }
        ModifiedArg { Inner: Expr, Modifier: ArgModifier, OutDecl: OutVarDecl?, Span }
        OutVarDecl { Name, AnnotatedType, Span }    // `out var x` 内联声明
        ↓
SymbolCollector:
        Z42FuncType { Params, Ret, RequiredCount, ParamModifiers? }
        ModifierMangling: 同 (Name, IsStatic, Arity) 组内 modifier 不同时启用
                         `Name$Arity$<modSig>` 注册键（class methods + free functions）
        ↓
TypeChecker:
        BoundModifiedArg { Inner, Modifier, OutDecl, Type, Span }
        BoundOutVarDecl { Name, Type, Span }
        TypeEnv.DefineParamModifier / LookupParamModifier
        - 修饰符一致性（CheckArgModifiers 12 处调用点）
        - lvalue 校验（IsLvalueForRef）
        - 严格类型匹配（CheckArgTypes 检测 BoundModifiedArg → exact equality）
        - LookupMethodOverload：modifier-tagged key 优先 → bare → arityKey
        - DA 扩展（FlowAnalyzer）：caller post-call + callee normal-return（throw 路径除外）
        - lambda 捕获禁止 + `in` 写保护（BindAssign 检测）
        - 占位拒绝：async / iterator + ref/out/in 参数（特性引入时再放开）
        ↓
IR Codegen（当前过渡期不变；follow-up spec 实施）：
        - 检测 BoundModifiedArg → emit LoadLocalAddr / LoadElemAddr / LoadFieldAddr
        - Call 指令携带 ref-aware 标志或新格式
        ↓
VM interp（当前过渡期不变；follow-up spec 实施）：
        - Value::Ref { kind: RefKind { Stack, Array, Field } }
        - frame.get/set 透明 deref 检测 Value::Ref
        - GC mark 包含 Value::Ref
```

**关键文件**：
- AST：`src/compiler/z42.Syntax/Parser/Ast.cs`（Param.Modifier / ModifiedArg / OutVarDecl）
- Mangling：`src/compiler/z42.Semantics/TypeCheck/Z42Type.cs`（`ModifierMangling` 静态类）
- 一致性检查：`src/compiler/z42.Semantics/TypeCheck/TypeChecker.Calls.cs`（CheckArgModifiers / IsLvalueForRef / LookupMethodOverload）
- DA：`src/compiler/z42.Semantics/TypeCheck/FlowAnalyzer.cs`（CheckDefiniteAssignment 接 functionParams）
- 写保护：`src/compiler/z42.Semantics/TypeCheck/TypeChecker.Exprs.Operators.cs`（BindAssign 检测 in param）
- env 修饰符表：`src/compiler/z42.Semantics/TypeCheck/TypeEnv.cs`（DefineParamModifier / LookupParamModifier）

**用户视角规范**：[parameter-modifiers.md](../language/parameter-modifiers.md)

---

## ManifestLoader（C1，2026-04-26）

位置：`src/compiler/z42.Project/`

**职责**：解析 `z42.workspace.toml` + member `<name>.z42.toml` 文件，
应用 workspace 共享继承，输出每个 member 的 `ResolvedManifest`（含字段来源链）。

### 核心模块

| 模块 | 职责 |
|---|---|
| `ManifestLoader` | 入口：发现 workspace 根（向上找）→ 加载 + 合并 + 校验 |
| `WorkspaceManifest` | `z42.workspace.toml` 数据模型（`[workspace.*]` / `[profile.*]` / `[policy]`）|
| `MemberManifest` | member `<name>.z42.toml` 数据模型，含 `FieldRef<T>` 表达 `xxx.workspace = true` 引用 |
| `ResolvedManifest` | 合并后最终配置 + `Origins` 字典（每字段记录来源） |
| `GlobExpander` | members 的 `*` / `**` 目录级 glob 展开 |
| `PathTemplateExpander` | 4 内置变量（`${workspace_dir}` / `${member_dir}` / `${member_name}` / `${profile}`）展开 + `$$` 转义 |
| `ManifestErrors` | 错误码工厂（WS003/005/007/010/011/020-024/030-039） |
| `IncludeResolver` | DFS 展开 include 链（C2） |
| `ManifestMerger` | 合并语义实现（C2） |
| `PolicyEnforcer` | workspace [policy] 锁定字段冲突检测（C3） |
| `PolicyFieldPath` | 字段路径解析 + fuzzy 建议（C3） |
| `CentralizedBuildLayout` | 集中产物路径派生（C3） |
| `MemberDependencyGraph`（z42.Pipeline，C4a） | 跨 member 依赖图 + DFS 三色环检测 + 拓扑层 |
| `WorkspaceBuildOrchestrator`（z42.Pipeline，C4a） | 串行编排 workspace 编译 + WS001/002/006 |
| `PackageCompiler.RunResolved`（C4a） | 接受 ResolvedManifest 的编译入口（供 orchestrator 调用） |
| `CliOutputFormatter`（z42.Driver，C4b） | ManifestException 友好渲染（颜色 + Reset/Bold/Dim） |
| `QueryCommands`（z42.Driver，C4b） | info / metadata / tree / lint-manifest 四个命令；MetadataDto JSON schema_version="1" |
| `ScaffoldCommands`（z42.Driver，C4c） | new / init / fmt 命令；含内联 Templates（Workspace / Gitignore / LibPreset / ExePreset / MemberManifest / 源文件骨架） |
| `BuildCommand.TryCleanWorkspace`（C4c） | clean 命令的 workspace 模式分派（集中清理 / -p per-member） |
| `IncrementalBuild`（z42.Pipeline，C5） | source_hash 增量编译查询：比对当前 .z42 hash 与上次 zpkg 记录，分流 cached / fresh；命中时复用 cache zbc bytes + 上次 zpkg 的 ExportedModules + Dependencies |
| `ZpkgBuilder.BuildPacked` cache zbc fullMode（C5） | packed 模式 cache zbc 用 ZbcFlags.None 写（含 SIGS / TYPE / EXPT / IMPT），让 ZbcReader.Read 单独反序列化为完整 IrModule；BuildIndexed 仍 stripped（zpkg.files[] 引用，VM 通过 zpkg 全局 SIGS 加载） |
| `ZpkgReader.ReadSourceHashes`（C5） | 跨 indexed/packed 读出每文件的 (sourceFile, sourceHash, namespace) 三元组，IncrementalBuild 用作命中判定 |

### 加载流程（C1+C2 范围）

```
ManifestLoader.LoadWorkspace(ctx, profile)
  ├─ 1. WorkspaceManifest.Load()
  │     - 强制文件名 z42.workspace.toml （WS030）
  │     - 检查 virtual manifest（WS036）
  │     - 解析 [workspace.project] 字段集合（WS033）
  │     - 解析 [workspace.dependencies] / [workspace.build] / [policy] / [profile.*]
  ├─ 2. GlobExpander.Expand()
  │     - members glob → 目录列表
  │     - exclude 过滤
  │     - 同目录两 manifest → WS005
  ├─ 3. default-members 校验（WS031）
  ├─ 4. 对每个 member：
  │     a. MemberManifest.Load()（含 WS003 段限制 + WS035 旧语法检测）
  │     b. ★ C2: IncludeResolver.Resolve()
  │           - DFS 展开 include 链 → preset 列表（按合并顺序）
  │           - 路径合法性（WS023/024） / 循环（WS020） / 深度（WS022）
  │           - PresetValidator 校验段限制（WS021）
  │     c. ★ C2: ManifestMerger.Merge(presets, self)
  │           - 标量后者覆盖 / 表字段级合并 / 数组整体覆盖
  │           - Origins 字典记录每字段的最终来源
  │     d. ResolveMember()：展开 .workspace = true 引用（WS032/WS034）
  │     e. PathTemplateExpander 应用到路径字段（WS037/038/039）
  │     f. ★ C3: CentralizedBuildLayout.Resolve()
  │           - workspace 模式 → dist/<member>.zpkg + cache/<member>/
  │           - 单工程 → fallback 到 member-local
  │     g. ★ C3: PolicyEnforcer.Enforce()
  │           - 默认锁定 build.output_dir / build.cache_dir / build.dist_dir
  │             (restructure-build-output-dirs, 2026-06-06)
  │           - 显式 [policy] 段（WS011 fuzzy 建议）
  │           - 仅检查 member 显式声明的字段（origin = MemberDirect/IncludePreset）
  │           - 一致 → Origins 标 PolicyLocked；冲突 → WS010
  │     h. 输出 ResolvedManifest + Origins
  └─ 5. orphan 检测（WS007 warning）
```

### 字段来源链（Origins）

`ResolvedManifest.Origins` 是 `Dictionary<string, FieldOrigin>`，记录每个字段的最终来源：

| OriginKind | 含义 | C 阶段 |
|---|---|---|
| `MemberDirect` | 由 member 自身 manifest 直接声明 | C1 |
| `WorkspaceProject` | 通过 `xxx.workspace = true` 引用 `[workspace.project]` | C1 |
| `WorkspaceDependency` | 通过 `xxx.workspace = true` 引用 `[workspace.dependencies]` | C1 |
| `IncludePreset` | 由 include 链中某 preset 提供 | C2 |
| `PolicyLocked` | 被 workspace `[policy]` 锁定 | C3 |

C4 的 `z42c info --resolved` 直接消费此字段输出"字段 + 来源 + 🔒 标记"。

### 与现有 ProjectManifest 的关系

- 现有 `ProjectManifest`（单工程 `.z42.toml` 路径）保留**不动**，行为与之前一致
- 新增 workspace 路径并行存在：CLI 入口判断"是否含 `z42.workspace.toml`"决定走哪条
- `ManifestException` 复用现有类型；C1 的工厂方法 `Z42Errors.*` 在 message 中携带 `WSxxx` 码 + 上下文，不破坏现有调用方

### 设计决策

详见 `docs/spec/archive/2026-04-26-extend-workspace-manifest/design.md`：

- **D1**：workspace 根文件名固定 `z42.workspace.toml`
- **D2**：virtual manifest 强制（root 不可兼任 member）
- **D3**：依赖语法对齐 Cargo（`xxx.workspace = true`）
- **D4**：members 支持 glob + exclude
- **D5**：`[workspace.project]` 仅 4 字段可共享（`version` / `authors` / `license` / `description`）
- **D6**：路径字段支持有限模板变量（4 内置只读，禁止用户自定义）

---

## TSIG 与跨包符号导入

### TSIG（Type Signature）section

zpkg 的 `TSIG` section 存储**包内所有 public 类型的结构化元数据**（类 / 接口 / 枚举 / 函数签名 / 约束），
供其他包 **编译期** 消费。运行时不使用 TSIG —— 运行时走 zbc 里的 `Function` + `TypeDesc`。

**形状（简化）：**

```csharp
sealed record ExportedModule(
    string                          Namespace,
    List<ExportedClassDef>          Classes,
    List<ExportedInterfaceDef>      Interfaces,
    List<ExportedEnumDef>           Enums,
    List<ExportedFunctionDef>       Functions
);
```

一个 zpkg 可含多个 `ExportedModule`（每个源文件一个）；同一包内不同 namespace 的源文件分别成组。

### TsigCache

位置：`src/compiler/z42.Pipeline/PackageCompiler.cs`

**职责**：编译器加载依赖包的 TSIG 元数据，按需供给 `ImportedSymbolLoader`。

**核心数据结构：**

```csharp
// namespace → list of zpkg full paths (2026-04-25 起改为 List<string>)
private readonly Dictionary<string, List<string>> _nsToPaths;
// zpkg path → 已加载 TSIG modules（懒加载 + 缓存）
private readonly Dictionary<string, List<ExportedModule>> _cache;
```

**加载流程：**

1. `ScanLibsForNamespaces`（在 `PackageCompiler.BuildTarget` 启动时）：扫描 `libs/` 下所有 `.zpkg`，
   读取每个 zpkg 的 `NSPC` section（namespaces 列表），对每个 namespace 调 `RegisterNamespace(ns, path)`。
2. `RegisterNamespace(ns, path)`：追加到 `_nsToPaths[ns]` 列表；**允许同一 namespace 多 zpkg 共享**。
3. `LoadForUsings(usings)` / `LoadAll()`：根据 `using` 声明（或全部已注册）聚合所有相关 zpkg 路径，
   首次访问时 `LoadZpkg` 解码 `TSIG` section，结果进 `_cache` 复用。

**libs 搜索路径（`BuildLibsDirs(projectDir, workspaceLibDirs)`）**：编译期扫描哪些目录找依赖 zpkg，按优先级：

1. project-local：`<projectDir>/libs`、workspace 布局 `<projectDir>/artifacts/build/libraries/<member>/<profile>/dist`、`artifacts/packages/<pkg>/libs`、legacy `artifacts/z42/libs` / `artifacts/libraries`；
2. 向上 walk-up：对每一级父目录重复上述 workspace / packages / legacy 候选（让 `scripts/` 下的工程也能解析仓库级 stdlib）；
3. **当前 workspace 兄弟成员（scaffold-z42c-selfhost, 2026-06-07）**：见下。
4. **`Z42_LIBS` fallback（opt-xtask-bootstrap-stdlib, 2026-06-04）**：若设置了运行时 lib 环境变量 `Z42_LIBS`，按平台路径分隔符拆分后**追加到末尾**（最低优先级）。

> 第 4 条让"编译期"也认 `Z42_LIBS`（此前只有 VM 运行期认它）。用途：CI 的 `xtask-bootstrap` composite 用 `install-z42` 下载预编译 stdlib 到 `.z42/libs`，直接 `Z42_LIBS=.z42/libs z42c build xtask.z42.toml` 编 xtask.zpkg，**不必从源码重建 stdlib**——`.z42/libs` 不在任何固定布局候选里，靠这个 fallback 命中。追加到末尾保证：本地/已构建 stdlib 的常规编译走布局扫描（行为不变），只有布局扫描找不到时才用 `Z42_LIBS`。

#### Workspace 兄弟成员解析（scaffold-z42c-selfhost dogfood #1, 2026-06-07）

**问题（根因）**：第 1/2 条的 workspace 布局把 `artifacts/build/libraries/` **硬编码**为唯一会扫描的 workspace 输出根。stdlib 兄弟依赖能解析纯属巧合——stdlib 恰好输出到那里。引入第二个 workspace（`src/compiler/` 自举编译器，输出 `artifacts/build/z42c/`）后，`z42c.syntax` 声明依赖 `z42c.core` 却扫不到 → `E0602: no loaded package provides this namespace`。

**修复（精准、隔离、零字节漂移）**：`WorkspaceBuildOrchestrator.Build` 收集**本 workspace** 全体成员的 `EffectiveDistDir`（排序去重），经 `CompileMember`（Func 第 3 形参）→ `RunResolved` → `BuildTarget` → `BuildLibsDirs` 的 `workspaceLibDirs` 形参透传；`BuildLibsDirs` 在第 1/2 条扫描**之后**、按**规范化 full-path 去重**追加，并**排序**（[common-pitfalls.md §1](../../../.claude/rules/common-pitfalls.md) 确定性——dirs 顺序喂给 first-wins nsMap / BuildDepIndex）。

效果：成员从**当前 workspace** 解析其 **`[dependencies]` 声明的**兄弟依赖（`ScanLibsForNamespaces` 的 `declaredDeps` 过滤未声明项），与该 workspace 输出位置无关。**零字节漂移保证**：已落在被扫描根的 workspace（stdlib），其成员 dist 早在 `dirs` 里 → 规范化去重后不新增条目、顺序不变 → nsMap / BuildDepIndex 内容与顺序不变。单工程构建 `workspaceLibDirs=null` → 行为完全不变。远程/下载依赖暂不支持（[self-hosting.md Deferred](self-hosting.md#deferred--future-work)）。

### 设计决策：namespace 可跨 zpkg（2026-04-25 vm-zpkg-dependency-loading）

**为什么**：对齐 C# assembly 模型 —— `System.Collections.Generic` 在 C# 里物理分布在 `System.Private.CoreLib`
和扩展 assembly。z42 stdlib 同样：`Std.Collections` 下的 `List<T>` / `Dictionary<K,V>` 驻留
`z42.core.zpkg`（实现 prelude 体验），而 `Queue<T>` / `Stack<T>` 驻留 `z42.collections.zpkg`。

**实现关键**：`_nsToPaths` 必须是 `Dictionary<string, List<string>>`（不是
`Dictionary<string, string>` + first-wins）；否则后来的 zpkg 被静默丢弃，导致
`QualifyClassName` 在用户代码引用 Stack / Queue 时找不到 namespace 映射、
IR 生成 bare 名 `Stack` 而非 FQ 名 `Std.Collections.Stack`，运行时 VCall 失败。

**对称性**：VM 侧 `LazyLoader` 同步支持多 zpkg 共享 namespace —— 两端必须一致。

---

## 符号解析与 QualifyClassName

### ImportedSymbolLoader

位置：`src/compiler/z42.Semantics/TypeCheck/ImportedSymbolLoader.cs`

**职责**：把 TSIG `ExportedModule` 列表合并为一组 `ImportedSymbols`，
供 `SymbolCollector` / `TypeChecker` 消费。

**关键字段：**
- `classes: Dictionary<string, Z42ClassType>` — 类名 → 重建后的类类型
- `classNs: Dictionary<string, string>` — 类名 → **简名到 namespace 的映射**（QualifyClassName 靠它）
- `interfaces`, `enumConsts`, `enumTypes`
- `classConstraints`, `funcConstraints` — L3-G3d 泛型约束（延迟解析到 bundle）

**冲突策略（first-wins）**：`if (!classes.ContainsKey(cls.Name))` —— 不同 zpkg 同名类保留先加载的。
预期同名冲突不应发生；若出现只在编译器层面 silent，运行时仍按 zbc func name 走。

### 两阶段加载（2026-04-26 fix-imported-type-two-phase-resolution）

`ImportedSymbolLoader.Load` 内部分两阶段处理类与接口，消除 self / forward
reference 的"未知名 → Z42PrimType 降级"。

```
Phase 1 — 骨架登记
  对每个 ExportedClass / ExportedInterface 创建空成员 Z42ClassType /
  Z42InterfaceType，仅填 Name + TypeParams + BaseClassName；登记进
  classes / interfaces 字典。Phase 1 内部不调 ResolveTypeName，
  没有跨类型 reference 解析。

Phase 2 — 成员填充（in-place mutate）
  对每个骨架，调 FillClassMembersInPlace / FillInterfaceMembersInPlace，
  解析 Fields / Methods 签名时通过 ResolveTypeName 在 Phase 1 完整字典里
  lookup —— 命中 → 返回真实 Z42ClassType / Z42InterfaceType；未命中 →
  Z42PrimType（真未知，TypeChecker 后续报错）。

  关键点：成员字典是 mutable Dictionary，Phase 2 直接 Add 进同一字典实例，
  不替换 Z42ClassType record。Phase 2 内 ResolveTypeName 拿到的 ClassType
  与最终 ImportedSymbols 输出的 ClassType 是同一引用 —— 字段填充后所有持有
  该引用的位置都看到完整 Fields / Methods（C# record 的 immutability 不
  影响 dict 内容）。
```

**为什么需要两阶段？**

旧实现单阶段：`RebuildClassType(cls)` 解析 cls.Fields 时，classes 字典还
没填到 cls 自己。`Exception.InnerException: Exception` 字段类型在
ResolveTypeName 里走 `_ => new Z42PrimType(name)` fallback，被降级。
下游 IsAssignableTo 比较 `Z42ClassType vs Z42PrimType` same-name 不通过 →
用户代码 `outer.InnerException = inner` 报 E0402。

按 [.claude/rules/philosophy.md "修复必须从根因出发"](../../.claude/rules/philosophy.md#修复必须从根因出发2026-04-26-强化)，
**禁止在 IsAssignableTo 加 PrimType↔ClassType 同名兼容分支**（症状级补丁）。
两阶段加载是经典 C# / Java 编译器的"先建骨架再填字段"做法，从源头物理消除降级。

#### intra-package 同名降级 fixup（2026-06-07 fix-array-indexed-method-e0402）

上面解决的是**跨 zpkg imported** 类型的降级。**本包内跨 CU** 的 forward
reference 有同样的病根但走不同代码路径：`SymbolCollector` 逐 CU 收集，
`ResolveType` 在解析某成员类型时若引用的类**所在 CU 还没被收集**（CU 收集
顺序非确定 —— common-pitfalls §1 文件枚举顺序），就降级为 `Z42PrimType(name)`。
单包 build 与 workspace build 可能落到相反的 CU 顺序，于是同一份源码在 workspace
fresh 构建报 `E0402: cannot assign T to T`、单包构建却通过（典型现场：
`z42.cli` 的 `ParseResult pr = this._parsers[i].Parse(rest)`，`ArgParser.Parse`
跨文件返回 `ParseResult`）。

修复 = `SymbolCollector.FinalizeTypeReferences()`（[SymbolCollector.TypeFixup.cs](../../../src/compiler/z42.Semantics/TypeCheck/SymbolCollector.TypeFixup.cs)）：
所有 CU 收集完后、在 `FinalizeInheritance()` 末尾跑一趟，把本包成员签名（方法
参数/返回、字段类型、free function、含 array/option/func/instantiated 嵌套）里
凡 `Z42PrimType(name)` 且 `name` 命中 `_classes` / `_interfaces` 的，**就地升级**
回真实 `Z42ClassType` / `Z42InterfaceType`（imported 名跳过，避免同名误升级）。
`MethodSymbol.Signature` / `FieldSymbol.Type` 为此加了 `internal set`（与
`ContainingType` 同样的两阶段构造模式）。同样**禁止**改 IsAssignableTo —— 这是
与上面 imported 两阶段对称的"一次性 fixup 物理消除降级"。

**ResolveTypeName 签名**：

```csharp
internal static Z42Type ResolveTypeName(
    string                                         name,
    HashSet<string>?                               genericParams = null,
    IReadOnlyDictionary<string, Z42ClassType>?     classes       = null,  // Phase 2 必传
    IReadOnlyDictionary<string, Z42InterfaceType>? interfaces    = null); // Phase 2 必传
```

Lookup 优先级：suffix (`[]` / `?`) > generic param > 内置 prim > classes > interfaces > Z42PrimType fallback。

### QualifyClassName

位置：`src/compiler/z42.Semantics/Codegen/IrGen.cs:58`

```csharp
string QualifyClassName(string className) {
    // local class shadows imported
    if (sem.Classes.ContainsKey(className) && !sem.ImportedClassNames.Contains(className))
        return QualifyName(className);
    return sem.ImportedClassNamespaces.TryGetValue(className, out var ns)
        ? $"{ns}.{className}" : QualifyName(className);
}
```

**作用**：ObjNew / Call / VCall 生成 IR 时把简名（用户代码写 `new Stack<int>()`）
resolve 到 FQ 名 `Std.Collections.Stack`，让 VM 能正确查 `func_index`。

**失败模式**：若 `sem.ImportedClassNamespaces` 没有该类（TsigCache 漏载、
TSIG 解码失败、namespace 过滤不含其 namespace 等），fallback 到
`QualifyName(className)`（用当前文件 namespace）→ 生成 bare 名 → 运行时
`function not found`。

---

## pseudo-class 策略与迁移

历史上 `Console` / `Math` / `Assert` / `Convert` / `String.*` 方法、`List<T>` / `Dictionary<K,V>`
由编译器的 **pseudo-class 机制**（`BuiltinTable.cs`）直接解析到 VM builtin，绕过 stdlib 加载。

**现状（2026-04-26）**：
- `Console` / `Math` / `Assert` / `Convert` / `String.*`：已迁移到 stdlib `.z42` 源码（L2 M7）
- `List<T>` / `Dictionary<K,V>`：已迁移到 `z42.core/src/Collections/`（L3-G4h step3）
- `StringBuilder`：已迁移到 `z42.text/StringBuilder.z42` 纯脚本实现
  （2026-04-26 script-first-stringbuilder）— 移除 IrGen `case "StringBuilder"`
  特例 + VM `__sb_*` 6 个 builtins + `NativeData::StringBuilder` 变体
- **残留兜底**：`SymbolCollector.cs:208-209` 仍有 `"List" => Z42PrimType("List")` 和
  `"Dictionary" => Z42PrimType("Dictionary")` 硬编码映射 —— 作为"TypeEnv.BuiltinClasses
  动态注入"未完成前的 bridge，计划在 L3-G 泛型类型表示扩展时清理（roadmap L2 backlog）
- **仅剩 pseudo-class**：`Array`（VM 原生 Value variant，不是 z42 class，不计入迁移目标）

---

## 跨 zpkg `impl` 块传播 — IMPL section + Phase 3 merge（2026-04-26 cross-zpkg-impl-propagation）

### 背景

L3-Impl1 让 `impl Trait for Type { ... }` 在同 CU 内工作（SymbolCollector 把
impl 方法合并进 target class 的 `Methods` 字典 + trait 加到
`_classInterfaces[target]`），但**跨 zpkg 不可见**：z42.numerics 给 z42.core
的 `int` 实现 `INumber<int>`，下游消费者读 z42.core TSIG 看不到这个 trait
→ `where T: INumber<int>` + `int` 类型实参编译报错。

### IMPL section（zpkg v0.8）

zpkg 加新 section `IMPL`：每个 ExportedModule 携带本 CU 的 `impl Trait for Type`
列表（仅 declarations，方法 body 仍走 MODS section）。

```
IMPL section layout
─────────────────────
[Module Count: u16]
For each module:
    [Namespace pool idx: u32]   ← 与 TSIG 模块顺序一致（positional matching）
    [Impl Count: u16]
    For each impl:
        [Target FQ name pool idx: u32]   ← 例如 "Std.Int32"
        [Trait FQ name pool idx: u32]    ← 例如 "Std.INumber"
        [Trait TypeArg Count: u8]
        For each trait type arg: [Type string pool idx: u32]
        [Method Count: u16]
        For each method: WriteMethodDef(...)   ← 复用 TSIG 的 MethodDef 序列化
```

**重要**：IMPL 模块顺序与 TSIG 模块顺序一致（positional matching），不能按
namespace 索引 —— 一个包内多个 .z42 文件可能共享 namespace（z42.core 的所有
文件都用 `Std`），按 namespace 唯一索引会撞键。

### ImportedSymbolLoader Phase 3

`ImportedSymbolLoader.Load` 由两阶段扩展为三阶段：

```
Phase 1 — 骨架登记                ← 已有
Phase 2 — 成员填充                ← 已有
Phase 3 — impl merge (NEW)
  foreach module.Impls:
    targetClass = classes[short_name(impl.TargetFqName)]
    foreach method in impl.Methods:
      targetClass.Methods.TryAdd(method.Name, method.Sig)   // first-wins
    classInterfaces[short_name].Add(impl.TraitName)         // dedupe
```

冲突策略：first-wins（与 SymbolCollector.MergeImported `TryAdd` 一致）。
`target` FQ 名通过 `SplitFqName` 拆 `Std.Int32` → namespace `Std` + short `int`，
仅在 namespace 匹配 `classNs[short]` 时才合并（避免不同包同名类污染）。

### IrGen — QualifyClassName 对齐 imported target

`IrGen.cs` 早先用 `QualifyName(targetNt.Name)` 给 impl 方法注册 funcParams
和生成方法 body 的 IR 函数符号。当 target 是 imported（如 z42.numerics 给
z42.core `int` 加方法），这会把方法生成到错误命名空间（`numerics.int.op_Add`
而非 `Std.Int32.op_Add`），导致 VM `func_index` 注册符号与消费者 VCall 期望
不一致。修复：改用 `((IEmitterContext)this).QualifyClassName(...)`，imported
target 走 source namespace，local target 行为不变（等同 QualifyName）。

### VM 端零改动

方法 body 走 z42.numerics 自己的 MODS section，函数符号 `Std.Int32.op_Add`。
当用户代码 `using z42.numerics`，lazy loader 注册该 zpkg 所有函数到 `func_index`，
VCall(int_obj, "op_Add") 通过 `primitive_class_name(I64) = "Std.Int32"` + method
拼出 `Std.Int32.op_Add` → 命中 z42.numerics body。VM decoder 不需要解析 IMPL
section（基于 tag 查找天然跳过未识别 section）。

### 兼容性

zbc version 0.7 → 0.8。pre-1.0 规则：旧 zbc 不可读，需要 `./xtask build test`
重生。

---

## 泛型接口 dispatch — Z42InterfaceType.TypeParams（2026-04-26 fix-generic-interface-dispatch）

### 背景

调用泛型接口方法时（`IEquatable<int>.Equals(T other)`），TypeChecker 必须
把方法签名里的 type-param `T` 替换成具体 TypeArg `int`，否则 arg 类型检查
会用未替换的 `T` 与实参 `int` 比较失败 → 报"argument type mismatch"。

C#/Java 的做法：泛型接口持有 TypeParams（声明列表）+ TypeArgs（实例化值），
dispatch 时构造 `TypeParams[i] → TypeArgs[i]` map，对方法签名做
`SubstituteTypeParams`。

### 关键结构

```csharp
public sealed record Z42InterfaceType(
    string Name,
    IReadOnlyDictionary<string, Z42FuncType> Methods,
    IReadOnlyList<Z42Type>? TypeArgs = null,            // 实例化时填
    IReadOnlyDictionary<string, Z42StaticMember>? StaticMembers = null,
    IReadOnlyList<string>? TypeParams = null);          // 声明列表
```

`TypeParams` 在所有构造点都必须从源头填入：

- `SymbolCollector.CollectInterfaces` — 从 `InterfaceDecl.TypeParams`
- `ImportedSymbolLoader.BuildInterfaceSkeleton` — 从 `ExportedInterfaceDef.TypeParams`
- `ExportedTypeExtractor.ExtractInterfaces` — 从 `Z42InterfaceType.TypeParams` 反向写入 TSIG
- `SymbolTable.ResolveGenericType` — 实例化时**保留 def.TypeParams**

### dispatch substitute

- `TypeChecker.BuildInterfaceSubstitutionMap(ifaceType)` — 由 TypeParams ↔
  TypeArgs 构造 substitution map
- `TypeChecker.Calls.cs` 接口方法调用分支：`imt → SubstituteTypeParams(imt, subMap)`
  再做 arg/ret 类型检查
- `TypeChecker.Exprs.cs` 接口属性 getter 同步替换返回类型

### 赋值兼容（TypeArgs-aware）

`Z42ClassType / Z42InstantiatedType → Z42InterfaceType` 路径不能仅按
**接口名** 比较 —— 必须比较 TypeArgs。`ClassImplementsInterfaceWithArgs`
通过 `_classInterfaces` 取出 class 实现的具体 `Z42InterfaceType`（含
TypeArgs），再用 `InterfacesEqual` 名 + 全部 TypeArg 类型比对。这避免
`class Foo : IEquatable<int>` 错误地被当作 `IEquatable<string>`。

---

## 多 CU 包内 symbol 共享（2026-04-26 fix-package-compiler-cross-file）

### 背景

`PackageCompiler` 编译一个 zpkg 时通常涉及多个源文件（CU）。同包内的 CU
互相引用（`z42.core/src/Exceptions/ArgumentNullException.z42` 继承
`Exceptions/ArgumentException.z42`，再继承 `Exception.z42`）。早期实现中
每个 CU 独立 `CompileFile`，`MergeImported` 仅加载 **外部已 build 的
zpkg**（`tsigCache.LoadAll()`），同包内未编译完的文件互不可见。开发者本地
有残留 zpkg cache 时偶然能跑通；清空 cache 后 stdlib 自启动失败：

```
ArgumentNullException.z42(8,44): error E0402:
  type `ArgumentNullException` has no member `Message`
```

### 两阶段编译

`PackageCompiler.TryCompileSourceFiles`（`src/compiler/z42.Pipeline/PackageCompiler.cs`）：

```
Phase 1 — 同包 declarations 预收集
  externalImported = ImportedSymbolLoader.Load(tsigCache.LoadAll(), allNs)
  sharedCollector  = new SymbolCollector(...)
  foreach cu in parsedCus:
    sharedCollector.Collect(cu, externalImported)   // Pass-0 only，无 body bind
  sharedCollector.FinalizeInheritance()             // 全局拓扑合并继承字段/方法
  intraSymbols = sharedSymbols.ExtractIntraSymbols(ns)  // 仅本包 declarations

Phase 2 — 完整编译
  combined = ImportedSymbolLoader.Combine(externalImported, intraSymbols)
             // intraSymbols 优先（本包覆盖外部同名）
  foreach cu in parsedCus:
    CompileFile(cu, combined)   // TypeCheck + IR + zbc，body binding 全做
```

### 关键不变量

- 每个 CU 的 sem 仍独立（body binding / IR 各自做）
- intraSymbols 仅含 declarations（class shape、interface methods sig、enum、
  free function sig），不含具体方法 body 或 IR
- 跨 CU 同名声明：first-wins（与现有 ImportedSymbols 合并语义一致）
- intraSymbols 不写入 zpkg TSIG（仅本次 build 使用，next build 从 zpkg 加载）
- 本包优先：`Combine(externalImported, intraSymbols)` 时 intraSymbols 覆盖
  externalImported 的同名条目（防止 stale zpkg 提供旧版 declaration）

### SymbolCollector.FinalizeInheritance

`Collect` 的 per-CU 第二阶段（`SymbolCollector.Classes.cs`）只能合并已经在
`_classes` 中的 base class 字段；当 CU 处理顺序与继承顺序不一致（按文件名
字母序：`Exceptions/ArgumentNullException.z42` 早于 `Exception.z42`），
派生类的 base 还没注册 → 合并被静默跳过 → intraSymbols 中派生类的 Fields
不含继承字段。多级继承（`ArgumentNullException → ArgumentException →
Exception`）会因任意一级断链而丢失 `Message` 字段。

`FinalizeInheritance()` 在 Phase 1 全部 CU 收集完成后做一次 **全局拓扑
合并**：递归 `FinalizeInheritanceOne(name)` 先处理 base 再合并 self，用
`done` HashSet 保证幂等。运行后 `_classes` 中每个 class 的 Fields/Methods
都已展开完整继承链，与单 CU + cached zpkg 路径一致。

### SymbolTable.ExtractIntraSymbols

`SymbolCollector` / `SymbolTable` 跟踪 `ImportedClassNames` /
`ImportedInterfaceNames` / `ImportedFuncNames` / `ImportedEnumNames`
（`MergeImported` 时 TryAdd 成功即记入），`ExtractIntraSymbols(ns, classNamespaces?)`
据此过滤掉外部 imported 的条目，只输出本包 declarations。返回的
`ImportedSymbols` 直接喂给 Phase 2 的 `Combine`。

**Per-class namespace（fix-generic-type-roundtrip 2026-04-28 引入）：**
同一个 package 可以包含多个 namespace（例：`z42.core` 同时含 `Std`、
`Std.Collections`、`Std.IO`）。`ExtractIntraSymbols` 接受可选
`classNamespaces` map（class 短名 → 该 class 实际声明所在 namespace），
按 class 单独写入 `ImportedSymbols.ClassNamespaces`；缺省时回退到
`ns` 参数（向后兼容单 namespace package）。

PackageCompiler 在 Phase 1 收集所有 CU 后扫描 `cu.Classes` 构建该 map：

```csharp
var classNamespaces = new Dictionary<string, string>(...);
foreach (var (_, _, cu, ns) in parsedCus)
    foreach (var cls in cu.Classes)
        classNamespaces.TryAdd(cls.Name, ns);
intraSymbols = sharedSymbols.ExtractIntraSymbols(firstNs, classNamespaces);
```

**为何重要**：`IrGen.QualifyClassName` 用 `ImportedClassNamespaces[name]`
拼 obj.new / vcall 的 fully-qualified target。错误的 namespace prefix（例：
`Std.KeyValuePair` vs 正确的 `Std.Collections.KeyValuePair`）会让 runtime
找不到类型，构造器静默不写字段、`.Value` 全是 null。修复前 z42.core
内 `Dictionary.Entries()` 调 `new KeyValuePair<K,V>(...)` 即触发该 bug；
修复后跨 namespace 同包引用走对前缀。

### 兼容性

- 仅在 PackageCompiler 路径生效；`SingleFileCompiler` 保持原样
- Cross-package 引用仍走 `tsigCache` 加载 zpkg（外部依赖不变）
- `intraSymbols` 不写盘，每次 build 重新计算

---

## DependencyIndex 与 Import 解析

位置：`src/compiler/z42.IR/DependencyIndex.cs`

**职责**：给定 `using Std.Collections;` 一条语句，TypeChecker 需要：
1. 确认该 namespace 存在（不存在 → 错误 Z1xxx）
2. 把该 namespace 标为"可见" → `ImportedSymbolLoader` 按此过滤 TSIG

`DepIndex`（`TypeChecker` 的构造参数）映射 namespace → 是否已知可用。
由 `ScanLibsForNamespaces` / `ScanZbcForNamespaces` 构建。

---

## Instance method binding（receiver-aware，2026-05-15 fix-instance-method-binding-receiver-aware）

位置：`src/compiler/z42.Semantics/Codegen/FunctionEmitterCalls.cs::EmitInstanceBoundCall`

实例方法调用 `receiver.Method(args)` 的绑定优先级（高 → 低）：

1. **Builtin collection 类型**（仅 Array 残留）：方法名是已知 builtin → 发 `BuiltinInstr`
2. **Receiver class 拥有该方法**：receiver 的类（或继承链上任一祖先）声明了 `Method` → 走 `v_call` 让 receiver 的 vtable dispatch
3. **DepIndex by name+arity**：上面都不匹配 → 在全局 dep 索引按方法名+arity 找到 imported stdlib 方法 → 发 `CallInstr`（静态调用 to qualified function）
4. **V_call fallback**：以上都没命中 → 默认 `v_call`（receiver 是 Unknown / 用户自定义类无 dep match）

### 为什么需要 receiver-aware 检查

DepIndex 按 `(method_name, arity)` 索引，**不区分声明类**。若多个类有同名同 arity 方法（例如 `Std.Toml.TomlValue.ContainsKey(string)` 与
`Std.Collections.Dictionary.ContainsKey(string)`），DepIndex 的 name-only 查找会返回任一命中，把 receiver 类的方法**劫持**到 stdlib 方法。

预 2026-05-15 的保护只是 `!ImportedClassNamespaces.ContainsKey(ReceiverClass)`——只挡 **class 名**冲突（user `class Stack` vs stdlib `Stack`），不挡 **method 名**跨类冲突。结果：当 stdlib 类（如 Dictionary）和用户类（TomlValue）都声明同名方法，user 类的方法被 stdlib 方法的 qualified function 替换发射，调用 user 类的实例进入 stdlib 函数体，触发"function not found"或访问错字段。

### `ReceiverChainHasMethod` 实现

```csharp
private bool ReceiverChainHasMethod(string receiverClass, string methodName)
{
    string current = _ctx.QualifyClassName(receiverClass);
    for (int depth = 0; depth < 32; depth++) {
        if (_ctx.ClassRegistry.TryGetMethods(current, out var methods)
            && methods.Contains(methodName))
            return true;
        if (!_ctx.ClassRegistry.TryGetBaseClassName(current, out var baseName) || baseName is null)
            return false;
        current = baseName;
    }
    return false;
}
```

`ClassRegistry` 含 local + imported 所有已加载类，由 `IrGen.Generate` 用
`SemanticModel.Classes` 注册时填入。继承链遍历是必要的——sub class 可能继承 base 的方法而不重写。

### 不影响的路径

- **Static 方法绑定**（`EmitStaticBoundCall`）：另一套逻辑，仍优先 DepIndex by `(receiverClass, method)` 二元 key（已经 receiver-aware），不变。
- **Virtual / interface 方法**（`EmitVirtualBoundCall`）：直接走 `v_call`，无 DepIndex 短路，不变。
- **Free function**（`EmitFreeBoundCall`）：无 receiver，DepIndex 路径合适，不变。

### 跨 zpkg 虚成员必须 VCall（2026-07-20 fix-crosspkg-virtual-override）

`ReceiverChainHasMethod`（z42c：`EmitContext.ChainHasMethod`）刻意**只认本地声明类**——
imported 类恒返 false，让 imported **非虚**实例方法（`_diags.Error()` 等）走 DepIndex 直呼
捷径（FQ `Call`，正确且更快）。但对 imported **虚**方法这是错的：接收者的运行时实际类型
可能是**子类 override**，直呼把基类实现编译期钉死 → override 永不生效。

典型：一个类 `override` 来自依赖 zpkg 的具体基类的虚方法后，经**基类静态类型**接收者调用
（`BuildHooks h = new ProjectHooks(); h.BeforeAssets()`）→ 跑基类 no-op 而非 override。
接口属性同源：`IPipelineContext ctx; ctx.Dirs` 落 `FieldGet "Dirs"` 打不到实现类
（`PipelineContext`）的 auto-property → 读到 Null。

**修复：imported 虚成员恒 VCall，交运行时按接收者实际 `type_desc.vtable` 派发。** 三条 emit 路径：

| 路径 | z42c 位置 | 判据 |
|------|-----------|------|
| 实例**方法**调用 | `ExprEmitter._emitCall` instance | `ReceiverMethodIsVirtual(recvType, m)` 真 → 跳 DepIndex 捷径走 VCall |
| 属性 **getter** | `ExprEmitter._emitMember` | 接口接收者 → 恒 `VCall get_X`（接口无字段，成员访问即属性读） |
| 属性 **setter** | `ExprEmitter._emitAssign` BoundMember | 接口接收者 → 恒 `VCall set_X` |

`ReceiverMethodIsVirtual`：imported 类经 TSIG 已把继承方法展开进 `OwnMethod*`（带 virtual/abstract
flag），查接收者自身 `Z42ClassType` 即命中「静态类型 = 基类本身」主场景，再沿 `BaseName` 走链兜
中间类。prim 接收者（`Type()` 非 `Z42ClassType`）→ false，仍走既有 prim 派发。self-host 7/7
byte-identical（z42c 自身无受影响调用点）→ blast radius 极小。见
[fix-crosspkg-virtual-override](../../spec/changes/fix-crosspkg-virtual-override/proposal.md)。

### 触发 spec

[docs/spec/archive/2026-05-15-fix-instance-method-binding-receiver-aware/](../../spec/archive/2026-05-15-fix-instance-method-binding-receiver-aware/)。
add-z42-json 加 JsonValue 类时与 TomlValue / Dictionary 多处 method 名共名暴露 bug，记录详细复现 + diagnose + 测试见 archive。

---

## Pratt 表达式解析

位置：`src/compiler/z42.Syntax/Parser/ExprParser.cs`

手写组合子（参考 Datalust/Superpower 设计），不引入外部 parser combinator 库
（为自举保留）。核心：

- **NudTable**（null denotation）：从哪个 token **开始** 表达式 —— 字面量、前缀运算符、`(`、`new`、lambda 等
- **LedTable**（left denotation）：在已有表达式后接什么 token —— 二元运算符、`.`（member）、`[`（index）、`(`（call）、`?:`、`switch`、postfix `++`/`--`

**优先级**（binding power）：见 `.claude/rules/compiler-z42c.md` 的 Pratt 表。

### Z42Type record 结构 equality（2026-05-03 fix-z42type-structural-equality）

C# `record` 默认 `Equals` 对 `IReadOnlyList<T>` 字段做**引用比较**而非元素级。
对于持 `IReadOnlyList<Z42Type>` 的类型 record 这是 bug —— 同结构两个不同
list 对象的实例报"不相等"，污染 `==`、HashSet/Dict key、`IsAssignableTo`
触底比较。

修复：[Z42Type.cs](../../src/compiler/z42.Semantics/TypeCheck/Z42Type.cs)
三个 record (`Z42InstantiatedType` / `Z42InterfaceType` / `Z42FuncType`)
override `Equals(SameType?)` + `GetHashCode()`，list 字段元素级递归
`Z42Type.Equals`。提供 `Z42Type.ListEquals<T>` 静态助手统一逻辑。

`Z42InterfaceType.Methods` / `StaticMembers` 字典字段仍走默认引用比较 ——
实践中 interface name 唯一确定其方法集，同名两次构造往往共享字典对象。
未发现 bug 前不引入字典深比成本。

`IsAssignableTo` 中既有的 `Z42FuncType` / `Z42InstantiatedType` element-wise
workaround 分支（line 63-74 / 82-86）保留作防御性 + 子类型放宽（`IsAssignableTo`
而非 `Equals` 递归）；删除评估留独立 cleanup spec。

### 嵌套 generic `>>` 拆分（2026-05-03 fix-nested-generic-parsing）

Lexer 把 `>>` 词法化为单一 `GtGt` token（shift-right operator）。`TypeParser`
解析嵌套 generic 如 `Foo<Bar<T>>` 时不能"两次 `Gt` 关闭"，需在 parser 端拆分。

**算法**（[TypeParser.cs](../../src/compiler/z42.Syntax/Parser/TypeParser.cs)）：
内部 `ParseInternal()` 返回一个 `ExtraClose: bool` flag。当 generic 关闭检查
遇到 `GtGt` 时 consume 整个 token，置 `ExtraClose=true` 上传；调用方收到则
视自己的关闭已被吸收（GtGt 用 1 个 `>` 关闭自己 + 1 个 `>` 关闭上层），不再
推进 cursor 也不处理 `[]` `?` 后缀（它们属于上层）。

**Depth-scan 站点**（5 处 lookahead helper：`SkipGenericParams` / `IsFieldDecl`
/ `IsLocalFunctionDecl` / 索引器扫描 / 局部变量扫描）用 `case GtGt: depth -= 2;`
计为两个关闭。

**为什么不在 lexer 拆分**：cursor 是 `readonly struct` + `IReadOnlyList<Token>`
的 immutable 设计（lookahead/backtracking 依赖此不变性）；由 parser 上下文
驱动 lexer mode 切换会污染分层。Roslyn 也用 parser-side split。

---

## 错误处理：Diagnostics vs Exceptions

z42 编译器同时使用**诊断收集**（`DiagnosticBag`）与**异常**（`ParseException` /
`CompilationException`），但二者各司其职 —— 不是冗余设计。新接手者最容易困
惑的就是"什么时候 `_diags.Error(...)` 后继续、什么时候 `throw`"，本节把
规则一次说清。

### 三个核心类型

| 类型 | 定义位置 | 作用 |
|------|---------|------|
| `DiagnosticBag` | `z42.Core/Diagnostics/DiagnosticBag.cs` | 收集多条诊断（错误 + 警告 + 信息），允许"先报告再继续"模式 |
| `ParseException` | `z42.Syntax/Parser/Parser.cs` | Parser 在**无法继续解析当前结构**时抛出（panic-mode）；携带 `Span` 与 `code` |
| `CompilationException` | `z42.Core/Diagnostics/DiagnosticBag.cs` | 由 `DiagnosticBag.ThrowIfErrors()` 抛出；表示"已收集到错误，停止当前 unit" |

### 使用规则

#### ① Parser 用 `ParseException`（强制中断）

**场景**：当 Parser 已经消费了一些 token，但接下来的 token 序列既不满足
任何已知文法规则、也无法智能猜测意图（例如 `int x = ;` 这种）→ 当前声明
/ 表达式必须放弃。

```csharp
// z42.Syntax/Parser/TopLevelParser.Helpers.cs
private static Token ExpectKind(ref TokenCursor cursor, TokenKind kind)
{
    if (cursor.Current.Kind != kind)
        throw new ParseException(
            $"expected `{Combinators.KindDisplay(kind)}`, got `{cursor.Current.Text}`",
            cursor.Current.Span,
            DiagnosticCodes.ExpectedToken);
    ...
}
```

**为什么不 `_diags.Error()` 后继续**：parser 是有状态的（cursor 位置），
错误 token 上"装作没事"会让后续解析被错误位置误导，产生雪崩式假错误。

#### ② Parser 顶层用 `catch (ParseException) when (diags != null)`（panic 恢复）

**只在顶层声明边界 catch**：`ParseCompilationUnit` 的 top-item 循环、
`ParseClassDecl` 的成员循环、`StmtParser` 的语句循环 —— 这些点适合
"放弃当前声明，跳到下一个边界继续 parse"，能在一次 build 里报告多个语法
错误。

```csharp
// z42.Syntax/Parser/TopLevelParser.cs
while (!cursor.IsEnd)
{
    try
    {
        // ... parse one top-item ...
    }
    catch (ParseException ex) when (diags != null)
    {
        diags.Error(ex.Code ?? DiagnosticCodes.UnexpectedToken, ex.Message, ex.Span);
        cursor = SkipToNextDeclaration(cursor);  // panic-mode: 找下一个 `class` / `void` / `using` 等开始 token
    }
}
```

**`when (diags != null)` 的意义**：API 暴露给"只 parse 不报错"的内部用法
（如表达式 fragment 解析）时，diags 为 null → 异常直接传播，调用方决定
是否兜底。这是有意的双模式接口。

#### ③ Semantic / Codegen 用 `_diags.Error()`（继续收集）

**场景**：类型检查、Codegen 阶段每个错误都不影响其他独立子树（例如方法 A
的类型错误不影响方法 B），所以全部走 diag 收集，跑完整个 CU 一次性报告。

```csharp
// z42.Semantics/TypeCheck/TypeChecker.Generics.cs
if (bundle.RequiresEnum && !IsEnumArg(typeArg))
    _diags.Error(DiagnosticCodes.TypeMismatch,
        $"type argument `{typeArg}` for `{typeParams[i]}` does not satisfy constraint `enum` on `{declName}`",
        callSpan);
// 注意：不 throw —— 继续检查下一个 type arg
```

**为什么不 `throw`**：让用户一次拿到所有错误，避免 fix-one-rebuild-find-next 循环。

#### ④ Body 绑定用 `try { ... } catch (CompilationException)`（函数级隔离）

**场景**：当一个函数 body 因深层错误彻底崩溃（不一定是 `CompilationException`，
也可能是任意 `Exception` 即 ICE），需要保护**其他函数继续被检查**。

```csharp
// z42.Semantics/TypeCheck/TypeChecker.cs
private void TryBindClassMethods(ClassDecl cls)
{
    try { BindClassMethods(cls); }
    catch (CompilationException) { /* 诊断已记录，继续下一个类 */ }
    catch (Exception ex) when (ex is not OutOfMemoryException and not StackOverflowException)
    {
        _diags.Error(DiagnosticCodes.InternalCompilerError,
            $"ICE while checking class `{cls.Name}`: [{ex.GetType().Name}] {ex.Message}", cls.Span);
    }
}
```

**`CompilationException` 不是错误**：它只是一个"我已经把错误塞进 diag 了，
请把我作为信号回到外层"。catch 时**不再追加诊断**（已经在 diag 里了）。

#### ⑤ Pipeline 用 `diags.HasErrors` 决策（不依赖异常控制流）

**场景**：`PipelineCore.Compile` 在调用 TypeChecker 后，先检查 `diags.HasErrors`
决定是否进入 IrGen；CompilationException 只是 IrGen 内部 `ThrowIfErrors()`
的兜底，正常路径都走 `if (diags.HasErrors)`。

```csharp
// z42.Pipeline/PipelineCore.cs
var sem = new TypeChecker(diags, feats, depIndex).Check(cu, imported);
if (diags.HasErrors)
    return new(null, diags, ...);   // 先看 diag，不进 IrGen
try
{
    var ir = new IrGen(...).Generate(cu);
    ...
}
catch (CompilationException)
{
    // 仅当 IrGen 内部 ThrowIfErrors 时触发，diag 已收集
    return new(null, diags, ...);
}
// 其他异常 = ICE，直接传播让 stack trace 暴露
```

### 决策表（速查）

| 场景 | 用什么 | 例子 |
|------|--------|------|
| Parser 期望 token X 但拿到 Y | `throw new ParseException` | `ExpectKind(LParen)` 失败 |
| Parser 表达式 / 语句中递归失败 | `throw new ParseException` | `Combinators.Unwrap(ref cursor)` |
| 顶层声明边界 / 成员循环 | `catch (ParseException) when (diags != null)` + 跳过 | `ParseCompilationUnit` top loop |
| TypeChecker 检测到类型不匹配 | `_diags.Error(...)` 继续 | `RequireAssignable` 失败 |
| TypeChecker 检测到未定义符号 | `_diags.Error(...)` 继续 | `BindIdent` 找不到 |
| Codegen 发现 unreachable 状态 | `_diags.Error(...)` 继续（让其他函数继续生成） | `IrGen` 内部检查 |
| Body 绑定包裹（保护其他函数） | `try / catch (CompilationException) / catch (Exception → ICE)` | `TryBindClassMethods` |
| Pipeline 阶段切换 | `if (diags.HasErrors) return` | `Compile` TypeChecker → IrGen 间 |
| 真正的内部 bug（NullRef 等） | **不 catch**，让它传播 | 任何未预期 `Exception` |

### 反模式（违规即修）

- ❌ Parser 内部 `_diags.Error()` 后继续 parsing —— 错误 cursor 位置会污染后续
- ❌ TypeChecker `throw new SomeException` 中断检查 —— 用户拿不到完整错误列表
- ❌ Pipeline 用 `try { ... } catch (Exception)` 吞掉 ICE —— 失去 stack trace 调试线索
- ❌ catch `CompilationException` 后再 `_diags.Error("...")` 加错误 —— 重复报告
- ❌ `_diags.Error(..., span)` 用了 `Span.Empty` / `(0,0)` —— 用户定位不到位置（`EmitImportDiagnostics` 之类的 cross-file 诊断除外，但应注释说明）

---

## Class field initializer pipeline（2026-05-02 fix-class-field-default-init）

实例字段 `int n = 5;` / 静态字段 `static int s = 1;` 的 init 表达式
统一在 TypeChecker 阶段绑定，按字段种类分流到 SemanticModel 的两本字典：

| 字段种类 | SemanticModel 字段 | 消费者（IrGen / FunctionEmitter） |
|---------|------------------|---------------------------------|
| `static` field with init | `BoundStaticInits` | `EmitStaticInit` 在 `__static_init__` 函数体中按拓扑序发射 `StaticSet` |
| 实例 field with init | `BoundInstanceInits` | 每个 ctor 入口（base ctor call 之后、用户 body 之前）按字段声明顺序注入 `FieldSet` |

**关键决策**：

- `TypeChecker.BindFieldInits` 同时绑定两类初始化器，类型检查时把字段类型作为 expected
  type（驱动数值字面量 / lambda / 重载选择），并在不可赋值时报错。
- `FunctionEmitter.EmitMethod` 接受 `IReadOnlyList<FieldDecl>? instanceFieldInits` 参数 —
  IrGen 在调用时仅对 ctor（`!isStatic && method.Name == className`）传入非空列表。
- 类没有显式 ctor 但本类或任一**本地祖先**有字段 init → IrGen 调
  `EmitImplicitCtor`，构造一个空 body 的 synth `FunctionDecl(name=cls.Name)`，
  通过 `CollectChainFieldInits` 沿祖先链 top-down 拼出整条链的 FieldDecl 列表，
  作为 `instanceFieldInits` 传给 `FunctionEmitter.EmitMethod`。
- 合成 ctor 不调用 base ctor —— 与 z42 现有"只有 `: base(...)` 才生成 base call"
  行为一致。跨 zpkg 祖先字段 init 不在合成范围内（FieldDecl AST 不可见），
  目前是已知限制。
- 未带显式初始化器的字段由 VM 端 `metadata::default_value_for(type_tag)` 在 `ObjNew`
  分配时填默认值，详见 `docs/design/runtime/vm-architecture.md` §ObjNew dispatch。

### 多 CU 共享 namespace 的 `__static_init__` 命名（fix-multi-file-static-init, 2026-05-15）

**问题**：原版 `EmitStaticInit` 直接用 `<namespace>.__static_init__` 作为函数名，
导致同 namespace 的不同 .z42 文件（例如 `Std/Platform.z42`、`Std/SubscriptionRefs.z42`）
都生成同名函数。运行时合并阶段（`merge_modules` 与 lazy_loader 的
`function_table`）按名字 dedupe（"first-wins on function name collisions"），
**只有第一个进入的 init 被执行**，其余文件的 static field 永远停留在默认值
（`null` / `0`）。

**症状**：第二个文件中 `public static class X { public static int Y = N; }` 的
`X.Y` 在运行时读取为 `null`，触发 "VM error" 或 silent assertion 失败 —— IR
显示 `static.set @<ns>.X.Y` 已正确发射，但运行时槽位从未被填充。

**修复**：

1. **编译器**（`FunctionEmitter.StaticInit.cs` + `IrGen.Generate`）：把 source-file
   stem 嵌进函数名 —— `<namespace>.<filestem>.__static_init__`，例如
   `Std.Platform.__static_init__` / `Std.SubscriptionRefs.__static_init__`。同
   namespace 多 CU 自动产生唯一名字，merge / lazy_loader dedupe 不再误删 init。
2. **Runtime lazy 路径**（`interp::init_static_fields` + `VmContext::collect_lazy_static_init_names`）：
   原本按 `<ns>.__static_init__` 单点 lookup，改为强制 load 所有 declared zpkg
   后枚举 `function_table` 中所有以 `.__static_init__` 结尾的函数名 → 全部跑。
3. **向后兼容**：`Generate(cu, sourcePath)` 的 `sourcePath` 是可选参数；测试
   helper 与 `--single-file` 路径不传，则退回到老式 `<ns>.__static_init__`，
   单 CU per namespace 行为不变。

**适用边界**：fix 只解决 "name collision in dedupe layer"。如果未来引入更激进
的合并优化（例如把多个 CU 的 BoundStaticInits 真正合并到单个 IR function），
要确保 topo-sort 仍然按 cross-CU 字段依赖工作 —— 当前每个 CU 的 init 独立排序，
跨 CU 字段依赖靠运行时按函数名字典序逐个调用来近似。

## 关键设计权衡

### 为什么 AST 节点是 `sealed record`

- **不可变 + 值相等**：便于并行分析、避免副作用
- **sealed**：匹配时 switch exhaustive，不漏 case
- **record**：自动合成 ctor / ToString / Equals，减少样板代码

### 为什么 TypeChecker 不直接写 IR

分离原因：
- TypeCheck 是**纯前端**（只看 AST + imports，产出 SemanticModel）
- Codegen 是**后端映射**（SemanticModel → IR）
- 两者通过 SemanticModel 接口解耦，便于将来加 LSP / incremental compilation

### 为什么不用 incremental parsing

z42 当前每次全量 parse。L3 后计划引入 LSP 时会考虑 incremental，但
bootstrap 阶段不做 —— 复杂度远超收益。

---

## Bound 树遍历统一框架（2026-05-10 introduce-bound-visitor）

### 背景

z42 的语义中间表示是 **Bound 树**（`Bound/BoundExpr.cs` + `Bound/BoundStmt.cs`），由 TypeChecker 产出、Codegen 消费。Bound 树的所有遍历都需要按节点种类分派——历史上每个 pass 各自手写 `switch (expr) { case BoundXxx: ... }`，导致：

- 加一个 `BoundXxx` 节点要同步改 5–15 个文件的 switch
- 漏改一处 → 编译器在该路径上静默退化（默认 fall-through）
- 同一巨型 switch 模式复制 5+ 处（`FunctionEmitter*` / `FlowAnalyzer` / `ClosureEscapeAnalyzer`）

### 解决方案：`BoundExprVisitor<T>` / `BoundStmtVisitor<T>`

`Bound/BoundExprVisitor.cs` 定义双层基类：

```csharp
public abstract class BoundExprVisitor<TResult>
{
    public TResult Visit(BoundExpr e) => e switch
    {
        BoundLitInt n   => VisitLitInt(n),
        BoundLitFloat f => VisitLitFloat(f),
        // ... 29 个 BoundExpr 子类对应 29 个 case
        _ => throw new InvalidOperationException("ICE — add a case to the base switch")
    };

    protected abstract TResult VisitLitInt(BoundLitInt n);
    // ... 29 个 abstract Visit 方法
}

public abstract class BoundExprWalker : BoundExprVisitor<Unit>
{
    // 默认实现：leaves 为 no-op，interior 节点递归子节点
    // 子类只 override 关心的节点
}
```

`Unit` 是 `record struct` 占位（C# 不允许 `void` 作为泛型实参）。`BoundStmtVisitor<T>` / `BoundStmtWalker` 同模式，覆盖 16 个 `BoundStmt` 子类。

### 为什么这是正面设计

**新增 BoundXxx 节点的工作流（5 步，编译器强制）**：

1. 在 `BoundExpr.cs` 加 `sealed record BoundFoo(...)`
2. 在 `BoundExprVisitor.Visit` switch 加一行 `BoundFoo x => VisitFoo(x)`
3. 加 `protected abstract TResult VisitFoo(BoundFoo x)`
4. `dotnet build` → 所有 visitor 子类编译期失败（必须 implement `VisitFoo`）
5. 在每个子类（含 Walker 默认）实现新方法

**第 4 步是核心保险**：基类 `abstract` 强制全员关注；不可能再发生"加节点漏改 dispatch"的静默退化。

### 当前消费者

| Pass | Visitor 子类 | TResult |
|------|------------|---------|
| `FunctionEmitter.EmitExpr` | `IrEmitExprVisitor` (nested) | `TypedReg` |
| `FunctionEmitter.EmitBoundStmt` | `IrEmitStmtVisitor` (nested) | `Unit` |
| `FunctionEmitter.CollectClassRefs` | `ClassRefScanner : BoundExprWalker` | `Unit` |
| `FlowAnalyzer.AlwaysReturns` | `AlwaysReturnsVisitor` (singleton) | `bool` |
| `FlowAnalyzer.AnalyzeStmt` | `DefiniteAssignmentVisitor` | `Unit` |
| `FlowAnalyzer.CheckReads` | `ReadsVisitor` | `Unit` |
| `ClosureEscapeAnalyzer` Pass 1/2 | `CandidateCollector` / `EscapeStmtScanner` / `EscapeExprScanner` | `Unit` |

### 实现要点

- **私有状态访问**：Codegen visitor 是 `FunctionEmitter` 的 `private nested sealed class`，通过 `_e: FunctionEmitter` 字段访问外层 `_locals` / `_ctx` / `Emit` / `Alloc` 等私有成员，保持封装零破坏
- **行为等价**：legacy switch 的"未覆盖节点 default fall-through" 必须显式 override 为 no-op，避免 Walker 默认递归引入新行为
- **状态化 Walker**：分支 isolation（如 `BoundIf` 的 then/else 各自 `HashSet<string>` 副本）通过 visitor 实例字段 push/pop 模拟
- **多次 Visit 复用**：emitter 持有 lazy-initialized `_stmtVisitor` / `_exprVisitor` 实例字段，避免每个 stmt/expr 重新构造

### 不变量（不得破坏）

- `BoundExpr` / `BoundStmt` 的子类层级保持**扁平**（不引入中间抽象类），否则 visitor 接口爆炸
- 新增 BoundXxx 节点 → 必须先改 `BoundExprVisitor` 基类（abstract + switch），不允许只在某个 pass 里临时 fall-through
- Visitor 子类不得绕过 abstract 用 `default → throw`（exhaustive 是设计核心，绕过等于丢掉编译期保险）

详见：[docs/spec/archive/2026-05-10-introduce-bound-visitor/](../../spec/archive/2026-05-10-introduce-bound-visitor/) 的 design.md。

---

## Symbol 层（2026-05-10 split-symbol-from-type）

### 背景

`Z42ClassType` 历史上把**类型身份**（Name / TypeParams / BaseClassName）和
**声明身份**（Fields / Methods / StaticFields / StaticMethods 字典，值类型 `Z42FuncType` / `Z42Type`）
混在一个 record 里。带来两个互相纠缠的问题：

1. **Decl 身份在 pipeline 中丢失**：AST `FunctionDecl` / `FieldDecl` 在 SymbolCollector
   之后被折叠进 `Z42ClassType.Methods` 字典（值是签名），原始 Decl 节点不再可达。
   BoundCall 只有字符串名 `MethodName` + `ReceiverClass`，零 back-pointer 到源 decl。
   Codegen 想知道方法的 [Test] 属性必须重新走 `cu.Classes` AST。
2. **缺 Symbol 层**：没有 `IMethodSymbol` / `IFieldSymbol` 等 Roslyn 风格 first-class
   符号对象。所有成员查询都通过 `cls.Methods["foo"]` 字典 lookup，63 个调用点散落。

### 解决方案：`Z42.Semantics.Symbols` 命名空间

引入三层接口 + 两个实现 record/class：

```
IMemberSymbol               base 接口
   Name, Span, Visibility, ContainingType: Z42Type?

IMethodSymbol : IMemberSymbol     方法接口
   Signature: Z42FuncType
   Modifiers: FunctionModifiers   ← single source of truth
   Decl: FunctionDecl?            ← 本地非空 / imported 为 null
   TestAttributes: IReadOnlyList<TestAttribute>?
   IsStatic / IsVirtual / ... 默认接口属性派生自 Modifiers

IFieldSymbol : IMemberSymbol      字段接口
   Type: Z42Type
   IsStatic, IsEvent
   Decl: FieldDecl?

实现：MethodSymbol / FieldSymbol（sealed class，不是 record，避免循环 Equals）
   手写 Equals/GetHashCode based on (ContainingType.Name 短名, Name, Signature/Type)
   Decl 是 back-pointer 不参与相等性
   ContainingType 是 internal 可设值（two-phase 构造解决 Z42ClassType.Methods
   持有 IMethodSymbol，IMethodSymbol.ContainingType 持有 Z42ClassType 的循环依赖）
```

### Z42ClassType 字典值类型切换

```
Z42ClassType.Fields         : IReadOnlyDictionary<string, IFieldSymbol>
Z42ClassType.Methods        : IReadOnlyDictionary<string, IMethodSymbol>
Z42ClassType.StaticFields   : IReadOnlyDictionary<string, IFieldSymbol>
Z42ClassType.StaticMethods  : IReadOnlyDictionary<string, IMethodSymbol>
Z42InterfaceType.Methods    : IReadOnlyDictionary<string, IMethodSymbol>
```

`Z42ClassType.Equals` override 仅按 `(Name, IsStruct)` 比对，避免循环递归
（如果 record 默认 Equals 包含 Methods 字典，MethodSymbol.Equals → Z42ClassType.Equals
→ 字典遍历 → MethodSymbol.Equals 死循环）。

### BoundCall vs BoundIndirectCall（Roslyn 风格分离）

历史上 `BoundCall` 同时承载：
- 直接方法分派（Static / Instance / Virtual / Free）
- lambda / 函数变量 / 闭包间接调用

split-symbol-from-type Phase 4 拆开两个节点：

```
BoundCall: 直接方法分派
   Kind: Free | Static | Instance | Virtual
   Symbol: IMethodSymbol?    ← 非空 on happy path（resolved 直接分派），
                              null only on error fallback
   Receiver, ReceiverClass, MethodName, CalleeName, Args, RetType

BoundIndirectCall: 间接调用（函数值）
   Callee: BoundExpr         ← lambda / ident-of-FuncType / member-of-FuncType
   Args, RetType
   （无 Symbol — 函数值不是方法引用）
```

BoundExprVisitor 加 `VisitIndirectCall` abstract → 5 个 visitor 子类编译期失败 →
强制全员 override（complete coverage 由 visitor 框架保证）。

### 不变量

1. **Symbol 是 sealed class（不是 record）** — 避免循环引用通过默认 record Equals 死递归
2. **Decl 是 back-pointer，不参与相等性** — 本地非空、imported / interface-abstract 为 null
3. **Modifiers / Span / TestAttributes 在 Symbol 字段** — single source of truth；本地构造
   时从 `decl.X` 拷贝；imported 直接传入（AST 不可变 + 构造时拷贝 → 永不漂移）
4. **ContainingType: Z42Type?** — class 成员是 Z42ClassType，interface 成员是 Z42InterfaceType，
   顶层自由函数为 null
5. **Z42ClassType.Equals 仅按 (Name, IsStruct)** — 不可恢复 default record equality
6. **BoundCall.Symbol 非空 on happy path** — error fallback 路径允许 null（PipelineCore 在
   diags.HasErrors 时 abort 不到达 codegen）
7. **BoundIndirectCall 永不携带 method symbol** — 间接调用语义上不是方法引用

### 当前消费者

| Pass | 通过 Symbol 的访问 |
|------|---------------------|
| TypeChecker (49 sites) | cls.Methods[name].Signature.{Params,Ret,...} |
| Codegen IrGen (TestIndex) | msym.TestAttributes（不再 walk AST） |
| TestAttributeValidator | msym.TestAttributes（数据等价 m.TestAttributes） |
| Codegen FunctionEmitter | cls.Methods[name].Signature for ctor 重载查找 |
| ExportedTypeExtractor | msym.Signature / fsym.Type 写 TSIG |

### 不解决的问题（follow-up spec）

- **顶层函数 wrapper** — 顶层自由函数仍存为 SymbolTable.Functions: Dictionary<string, Z42FuncType>，
  不是 IMethodSymbol。原因：scope 控制；spec extension 留 follow-up
- **IPropertySymbol / IParameterSymbol / INamespaceSymbol / IEventSymbol** — full Roslyn shape
  留 follow-up `extend-symbol-layer`
- **R-series 反射 API surface** — 本 spec 准备基础设施；具体 reflection API 留各 phase spec

详见：[docs/spec/archive/2026-05-10-split-symbol-from-type/](../../spec/archive/2026-05-10-split-symbol-from-type/) 的 design.md。

---

## 方法重载决议：type-based mangling + 协议豁免名单（add-type-based-overloads，2026-07-01）

### 背景

z42c 早期方法身份只按 `Name$arity` 编码（arity-only）。两个同名同 arity 但形参类型不同的
方法（如 `F(int)` 与 `F(string)`）注册到同一个键，`SymbolCollector` 的 `ct.Methods.Put`
first-wins 语义下后者方法体被静默丢弃、零诊断——`z42.test/Assert` 的 5 组 `long`/`double`
重载曾因此静默失效。本节记录最终修复后的机制。

### 签名键派生（`OverloadResolver.MangleKey`）

方法注册键三档，**只有第三档改名**（前两档 byte-identical 于本变更前）：

| 档位 | 触发条件 | 注册键 |
|------|---------|--------|
| 唯一方法 | 同名只有 1 个 | `Name`（不变） |
| arity-distinct 重载 | 同名多个但 arity 不同 | `Name$arity`（不变） |
| **同 (name,arity) ≥ 2 重载** | 同名同 arity 出现 ≥2 次 | `Name$arity$T1$T2...`（Ti = 每个形参类型的签名键片段） |

`OverloadResolver.MangleKey(name, paramTypes[], paramCount)`（`OverloadResolver.z42:30`）派生第三档键；
每个形参类型片段由 `TypeKey` 产出：`Z42Type.Canon(t.Name())` 归一（基本类型别名 `int→i32`
等 + 剥 `?` nullable 后缀）后剥空白（泛型名含 `", "`）。**Canon 归一是关键**——`F(int)` 与
`F(i32)`、`G(string)` 与 `G(string?)` 归一后是同一个键，视为重复声明而非合法重载
（见下"重复重载检测"）。

**跨包一致性**：TSIG 每个方法记录本就携带每参数 `TypeName`，`ImportedSymbolLoader` 从 imported
TSIG 用同一个 `MangleKey` 重算注册键 → 跨 zpkg 调用方无需额外元数据、无 zbc/zpkg 格式 bump。
但**不对 imported 方法的 `RegKey` 做重算覆盖**——`sym.RegKey = m.Name` verbatim 读回定义点写入的
键，因为 `_hybridTypeName` 与 `Canon(Name())` 两套类型名生成逻辑不保证逐字符一致，重算可能产出
与定义点不同的键、派发断裂（见 `ImportedSymbolLoader.z42`）。

### 决议算法（`OverloadResolver.Resolve`）

调用点候选集（同名所有重载，沿 base 链聚合）经三步择优：

1. **适用性**（`_applicable`）：arity 匹配 + 每个实参可赋值到对应形参（含子类→基类 /
   prim→object 装箱 / 接口实现）
2. **最具体**（`_betterThan` 偏序）：每个位置不差、至少一处严格更优的候选胜出；
   精确类型匹配 > 加宽/装箱；v1 不做 `int→long→double` 完整隐式数值转换排序表，
   多个加宽候选并列即报歧义（`AmbiguousOverload`），用户显式 cast 消歧
3. **零 / 一 / 多**：零适用 → no-match 诊断；恰一个"支配所有其它候选" → 选中；
   零个或多个"支配所有" → 歧义诊断

`TypeChecker._resolveOverload`（`TypeChecker.z42:1651`）包装候选枚举 + `OverloadResolver.Resolve`
调用 + 诊断落地，是 24 处调用点（静态/实例/自由函数/原始类型接收者/泛型实例化/操作符）的统一入口。
遗留的 `_overloadKey`/`_findMethod`（纯 arity 键查找，`TypeChecker.z42:1625`）在个别调用点仍存在，
**新调用点一律走 `_resolveOverload`**，历史调用点迁移视具体场景独立评估（如 `_bindBinary` 已于
2026-07-01 完成迁移，见下）。

### 实例方法扩展 + 协议豁免名单（2026-07-01）

静态方法落地 type-mangle 后，用户要求扩展到**实例方法**。核心约束：VM `exec_vcall.rs` 的
`vtable_index` 对**部分方法名做字面量硬编码查找**（不查经决议后的 mangled 键），若这些名字被
mangle 掉，VM 侧查找必然落空。逐名核实约束来源（不是笼统"协议方法都不能 mangle"）：

| 方法名 | 约束来源 | 证据 |
|--------|---------|------|
| `ToString` | **VM 硬编码**：Rust 侧对该名字做字面量 `vtable_index.get("ToString")` | `well_known_names.rs:69`（`METHOD_TO_STRING`）、`dispatch.rs:90`、`jit/helpers/value.rs:134` |
| `Equals` / `GetHashCode` / `GetType` | **非 VM 硬编码**——编译器侧 `DependencyIndex._isProtocol` 因每个类都从 `Object` 继承这三者，若参与跨包裸名索引会造成全局碰撞，故排除出 cross-package 依赖索引 | `DependencyIndex.z42:126`（`_isProtocol`） |
| `get_Item` / `set_Item` | **TypeChecker 自身硬编码**：用字面量字符串 `ct.Methods.ContainsKey("get_Item")` 查找，绕开 `_resolveOverload`/`_overloadKey`；VM 侧亦有对应硬编码 | `TypeChecker.z42:621-622, 745-749, 911-916` |

**`op_*`（操作符）不在豁免名单内**——它们本就是**静态**方法（`_bindBinary` 构造
`BoundCall("static", ...)`），与本节"实例方法 VM 裸名派发"约束无关，天然已被既有静态
mangle 规则覆盖，不需要额外豁免。

最终协议豁免名单固定为 `SymbolCollector.IsProtocolExempt`（`SymbolCollector.z42:396`）：

```z42
public static bool IsProtocolExempt(string name) {
    return name == "ToString" || name == "Equals" || name == "GetHashCode" || name == "GetType"
        || name == "get_Item" || name == "set_Item";
}
```

`_fillClass` 的实例方法注册分支（`SymbolCollector.z42:507`）：

```z42
bool wantMangle = arityDup.ContainsKey(aritK) && arityDup.Get(aritK) == "2"
    && (mst || !SymbolCollector.IsProtocolExempt(md.Name));
```

即：静态方法（`mst`）无条件按 `(name,arity)≥2` mangle；实例方法额外要求不在豁免名单内。
豁免名单内的方法沿用现状 arity-only 注册（first-wins，不引入新行为——`String.Equals(object?)` /
`Equals(string)` 这类碰撞是已知、已接受的历史行为，不在此处修）。

**virtual/override 安全性（`_passFixupOverrides` 修复，2026-07-01 阶段 8）**：`_fillClass` 的
`wantMangle` 判定只看**当前类自身**的 `arityDup`——若 Base 声明两个同 arity 的 virtual 重载
（如 `Handle(int)`/`Handle(string)`）会各自 mangle 出 `Handle$1$i32`/`Handle$1$string`；但
Derived 若只 `override` 其中一个，Derived 本地该 arity 只出现 1 次（`arityDup=1`）→ 不满足
mangle 条件 → 注册成裸名 `Handle`。VM 侧 vtable 合并（`TypeDesc::derive_simple_method_name` /
`merge_with_base`，`loader.rs`）按 `simple_name` **字符串裸匹配** slot——两端键不一致时，
Derived 的 override 会落进一个**新**槽位，Base 的 `Handle$1$i32` 槽位原样保留，虚派发仍打到
Base 实现，override 语义失效。"签名一致 ⟹ mangled 键自然一致"这一假设**不成立**——mangle 与否
取决于本地 arity 出现次数，不是签名本身的函数。

修复：`SymbolCollector._passFixupOverrides`（`SymbolCollector.z42`，`_passMembers` 之后、
`_passImpls` 之前跑，`CollectAll` 路径额外要求所有 CU 的 `_passMembers` 全部跑完才能跑，因为跨
CU 场景不保证 base 类先于 derived 类被处理）对每个带 `override` 修饰符的方法，调用
`_findVirtualOrigin` 沿 base 链上溯 AST `Decl.Mods`（而非依赖 `baseCt.Methods` 是否已被本 pass
修正，从而与跨 CU 处理顺序无关），找到该虚方法**最初**（非 override）声明所在层的 `RegKey`
作为权威键，把 override 方法的 `RegKey` 就地改写为该键（`MethodSymbol.RegKey` 可变字段）。
imported 基类方法无 AST `Decl`（`HasDecl=false`）视为上溯终点。改写后 `ct.Methods` 中旧的裸键
条目会残留（`StrMap` 无 `Remove`），但两键指向同一个（已改写 RegKey 的）`MethodSymbol` 对象，
下游 `_collectOverloads` 按 `RegKey` 去重会自然合并，不产生重复候选。

**VM 零改动**：`vtable_index` 的 key 派生自 z42c 写入 zpkg 的 qualified 函数名
（`TypeDesc::derive_simple_method_name`），是否 mangle 完全由 z42c 端的协议豁免名单决定，
Rust 侧逻辑不变——唯一约束是 z42c 永不能把豁免名单内的名字 mangle 掉。

### 重复重载检测（`TypeChecker._checkDuplicateStaticOverloads`）

`SymbolCollector` 的 `Put` 是 first-wins：两个签名若 `Canon` 归一后撞出同一个 mangled 键
（如 `F(int)` vs `F(i32)`，或 `G(string)` vs `G(string?)`），后者方法体会被无声覆盖。
`TypeChecker._bindClass` 在绑体前调用 `_checkDuplicateStaticOverloads`（`TypeChecker.z42`）
重放一遍碰撞检测，命中即报 `E0408 DuplicateDeclaration`：

- 静态方法与实例方法分桶检测（`_dupCheckKey` 用 `s$`/`i$` 前缀区分，同名静态+实例方法本就不
  共享注册表，不应互相误判）
- 协议豁免名单内的方法名（`SymbolCollector.IsProtocolExempt`）跳过检测——它们本就不参与
  mangle，多个同 arity 声明是已接受的 first-wins 行为，不是新引入的 bug
- **设计裁定**：不支持 nullable-only / alias-only "重载"（视为重复声明而非合法重载），不为
  兼容这种写法改 mangle 方案

### `_bindBinary` 迁移到 `_resolveOverload`（2026-07-01，独立 bug 修复）

调研实例方法扩展时发现：`_bindBinary`（`TypeChecker.z42`，处理 `a + b` 等二元运算符重载分派）
仍用静态重载那一期遗留的旧 `_overloadKey`/`_findMethod`（纯 arity 键查找），未跟随其余 23 处
调用点迁移到 `_resolveOverload`。`op_*` 方法本身早已因静态 mangle 规则获得 mangled 注册键，
但 `_bindBinary` 用 arity-only 键去查，查不到 → 同 arity 多重载操作符（如 `op_Add(Vec,Vec)`
与 `op_Add(Vec,int)`）派发失败。已迁移：

```z42
MethodSymbol opMs = this._resolveOverload(env.Symbols, lct, opMethod, opArgs, 2, bin.Span);
if (opMs != null) {
    return new BoundCall("static", false, null, lct.Name(), opMs.RegKey, opArgs, 2, opMs.Signature.Ret, bin.Span);
}
```

测试：`src/tests/operators/operator_overload_multi_arity.z42`（端到端）+
`Z42cSemanticsTypeCheckTests.test_operator_overload_multi_arity_dispatch_selects_correct_overload`（单元）。

---

## 延伸阅读

- `docs/design/compiler/compilation.md` — 构建流程（manifest → zpkg 的用户视角）
- `docs/design/runtime/zbc.md` — `.zbc` 二进制格式
- `docs/design/language/namespace-using.md` — namespace / using 的语言规则
- `.claude/rules/compiler-z42c.md` — z42c 编译器开发规范（子包结构 + Lexer / Parser / AST 约定）

---

## Deferred / Future Work

> 索引也存于 [docs/roadmap.md](../../roadmap.md) "Deferred Backlog Index"。

### ~~compiler-future-typed-overload-resolution~~ ✅ 已修复 (2026-07-01)

**已修复**：`add-type-based-overloads` 变更落地 type-based mangling（`OverloadResolver.MangleKey` +
`Resolve`）+ 实例方法协议豁免名单，机制详见本文件"[方法重载决议：type-based mangling + 协议豁免名单](#方法重载决议type-based-mangling--协议豁免名单add-type-based-overloads2026-07-01)"一节。
`BinaryReader(byte[])`/`BinaryReader(Stream)` 一类同名同 arity 不同类型的构造器/方法此前需要靠
static factory 改名 workaround，现可直接声明为重载、按实参类型正确决议。

**原始描述**（历史存档）：投放 stdlib 期间反复触发的同名同元 ctor 冲突（StringWriter 早期
`Write(char)` / `Write(string)`、JSON/TOML/YAML `Parse(string)` / `Parse(Stream)`、
BinaryReader `(byte[])` / `(Stream)`、BinaryWriter `(int)` / `(Stream)`）。当时 C# bootstrap
编译器的 `SymbolCollector.Classes.cs:230-264` 用 `{Name}${Params.Count}` 编码 method/ctor
键，同名同 arity 不同类型的签名会撞键、后写者覆盖前者，只剩一个候选存活——2026-05-24 探索后
认定需要独立 spec 处理，2026-07-01 由 z42c 自举后的 `add-type-based-overloads` 变更彻底解决。

### ~~compiler-future-vcall-base-class-fallback~~ ✅ 已修复 (2026-05-26)

**已修复**：fix(compiler+vm): cross-zpkg subclass ctor + .base metadata + vtable fallback (commit a7c9c18d)

三处协同修复：
1. **IrGen.Classes.cs**：`EmitClassDesc` 改用 `QualifyClassName` 生成 `.base` 元数据，跨 zpkg 子类（如 `Demo.Ext.Dog : Demo.Base.Animal`）不再错误地写入当前模块命名空间
2. **FunctionEmitter.cs**：base ctor 调用名从 `SemanticModel.Classes + QualifyClassName` 推导，不再依赖 `ClassRegistry`（后者以 `QualifyName` 存储 key，跨 zpkg 时 key 错误）
3. **exec_vcall.rs**：lazy hierarchy walk 的 `or_else` 分支改为对超过第一层的 base 类调用 `ctx.try_lookup_type()`，解锁跨 zpkg 多级继承的 VCall fallback

测试：`src/tests/cross-zpkg/vcall_base_fallback`（Dog 未 override Breathe → 验证 dispatch 到 Animal.Breathe）

**原始描述**：add-z42-net K1 (2026-05-24) — `NetworkStream extends Std.IO.Stream`，未 override `Seek` / `Length` / `Position` 等 base virtual method 时 VCall 抛 "function not found"。



- **来源**：[docs/spec/archive/2026-05-10-docs-review/review.md](../../spec/archive/2026-05-10-docs-review/review.md) Part 2 §2.1（推荐引入 `BoundExprVisitor<T>` / `BoundStmtVisitor<T>` 抽象基类）；2026-05-07 探索后暂缓
- **触发原因（探索结论）**：
  1. 当前 `EmitExpr` / `EmitBoundStmt` switch 已经**部分 visitor 化**——FunctionEmitterStmts.cs 16 个 case 几乎全是单行 helper 调用；FunctionEmitterExprs.cs 28 个 case 中约 16 个已委托给 `EmitBoundXxx` helper。"巨型 switch" 实际负担没有 review.md 初判那么重
  2. **本 spec 单独做收益小**：抽象基类的"加新节点编译期穷尽性保证"价值需多消费者才显现。当前有 6 处 `case Bound` 站点（4 在 Codegen + FlowAnalyzer + ClosureEscapeAnalyzer），如果只迁 FunctionEmitter 一个，单消费者用 visitor 框架是过度设计
  3. **不解决 P0 LOC 问题**：visitor base class 只迁移 switch 体到 override 方法。FunctionEmitterExprs.cs 878 LOC 仍 ~880（每个 case 体仍要写）。review.md §1.1 P0 硬限违规需 `split-large-codegen-files` 解决
  4. **C# 多继承摩擦**：FunctionEmitter (sealed partial) 只能继承一个 base class，Exprs / Stmts 两套 visitor 不能直接同时继承——需 nested visitor 或继承一个+方法表 mix
- **前置依赖**：第二个 BoundExpr 消费者出现（dump-ast / lint / 第二个 emitter），让 visitor 抽象基类有真实使用者；或与 `split-symbol-from-type`（review.md §2.3 + §3.1）合并做（Symbol/Decl 分离时连带重审 visitor）
- **触发条件**：
  - dump-ast / 某个新分析器需要遍历 BoundExpr，引入即第二个真实消费者
  - 或者新增 BoundXxx 节点导致多处 switch 漏改的事故出现 ≥ 1 次
  - 或者 `split-symbol-from-type` spec 启动时一并设计
- **当前 workaround**：保持 FunctionEmitterExprs / Stmts 现有方法表风格不变；EmitExpr 主 switch 紧凑度可接受。后续 `split-large-codegen-files` 处理 FunctionEmitterExprs.cs 878 LOC 超限（按表达式类别拆 partial 文件）

