# z42c.analyzers

## 职责
编译期 **analyzer**：第三方（或本仓）写的诊断规则，在编译时遍历源码 AST、报告诊断（可附修复）。本包提供 analyzer 的**契约**（作者实现的接口与数据类型）、外部 analyzer zpkg 的**加载**、AST 遍历**驱动**，以及 `[lints]` 清单段的 severity **决策**。

**只读 AST、只产诊断**：不依赖语义层（`z42c.semantics` / `z42c.emission`），不改源码结构（那是 generator 的事）。在包编译里的位置是**前置线 ④**——generator 之后、包级编译之前，遍历 union CU（含 generator 产出），见 [source-compile.md](../../../docs/internals/src/compiler/source-compile.md)。

## 功能索引
命名空间 `Z42.Analyzers`。

| 功能 | 入口 |
|------|------|
| analyzer 契约 | `Analyzer`（`SupportedRules` / `ObservedKinds` / `OnSyntaxNode`）· `DiagRule` · `DiagSink` · `SyntaxKind` · `AnalyzerSeverity` · 修复面 `FixSink` / `CodeFix` / `TextEdit`（`src/Analysis.z42`）|
| 外部 zpkg 加载 | `AnalyzerLoader.Load(zpkgPaths, n) → Analyzer[]`：反射投影发现 `: Analyzer` 类型 + `ModuleLoader.Load` 使其可执行 + 实例化 |
| 驱动 | `AnalyzerDriver.Run(analyzers, n, cu, bag, lints, fix)`：遍历 CU，对命中 `ObservedKinds` 的节点回调；诊断经抑制（`SuppressionSet`）与 severity 决策后汇入 `DiagnosticBag`；`fix=true` 时把修复就地写回源文件 |
| `[lints]` 决策 | `LintConfig.Resolve(rule) → AnalyzerSeverity`（`EnabledByDefault` 门 → 精确 Id / `pkg.*` 通配覆盖 → `"none"` 抑制 → `warnings_as_errors` 升级）|

## 基础用法
写一个 analyzer（作者侧，`kind = "analyzer"` 工程）见 [compile-time-extensions.md](../../../docs/reference/src/toolchain/compile-time-extensions.md)。编译器侧由 `z42c.pipeline` 的 `PackageCompile` 调用：

```z42
Analyzer[] as = AnalyzerLoader.Load(paths, n);
AnalyzerDriver.Run(as, as.Length, cu, bag, lints, false);
```

## 如何测试验证
```bash
./xtask test compiler   # tests/analyzer（驱动 / [lints] / #suppress / --fix）；外部 zpkg 加载端到端在 z42c.pipeline/tests/pkgcompile
```

## 关联文档
- 包编译里的位置（前置线 ④）：[source-compile.md](../../../docs/internals/src/compiler/source-compile.md)
- `#suppress` / `[Suppress]` 与 attribute 分流：[attribute-pipeline.md](../../../docs/internals/src/compiler/attribute-pipeline.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Analysis.z42` | 契约：纯接口 + 数据类 + int 常量（无 delegate——命名 delegate 跨 zpkg 会丢 FQN）|
| `src/AnalyzerDriver.z42` | AST 遍历 + 分派；`DiagSinkImpl`（同时实现 `FixSink`）把 `DiagRule` 诊断映射进 `DiagnosticBag`；从 `cu.SuppressRegions` 建 `SuppressionSet`，walk 期间 push/pop `[Suppress]` 声明 |
| `src/AnalyzerLoader.z42` | 外部 zpkg 两路加载：Path-A `AssemblyLoadContext.Default().Load → GetTypes → GetInterface("Analyzer")` 发现 FQN；Path-B `ModuleLoader.Load` 使可执行 → `Type.GetType` + `Activator.CreateInstance` + `as Analyzer` |
| `src/LintConfig.z42` | `[lints]` severity 决策器（由 `z42c.pipeline` 的 `ManifestKnobs` 从清单中性字段构造）|

## 依赖关系
`z42c.core`（Span / Diagnostic）、`z42c.syntax`（AST 节点 / `CompilationUnit` / `SuppressionSet`）、`z42.io`（`--fix` 写回源）。被 `z42c.pipeline` 依赖。
