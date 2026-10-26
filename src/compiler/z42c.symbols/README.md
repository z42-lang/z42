# z42c.symbols

> 符号世界：收集 / 符号表 / 导入装载 / 类型模型 / 布局——自 z42c.semantics 拆出（semantics-split）。
> 命名空间 `Z42.Symbols`（与物理包一致，消费方的 `using Z42.Symbols` 即声明来源）。对 Binding 的活引用 0 处（#1351/#1355 清零）。

## 职责
符号收集（SymbolCollector 及各簇 pass）、符号表（SymbolTable 4 碎片）、导入符号装载（ImportedSymbolLoader + TsigReconcile 世界）、类型模型（Z42Type 家族）、类型代换/转换分类/基元模型/泛型约束/类型实参推断、struct 布局（StructLayout + Query 分片）、常量求值（ConstEval）、类级访问强制（TypeRefAccess）。

## 功能索引
| 功能 | 入口 |
|------|------|
| 符号收集 | `SymbolCollector.CollectAll`（各簇 pass 见 `Symbols/StubCollector` 等） |
| 类型解析 | `SymbolTable.ResolveTypeP` / `GetClass` / `GetInterface` |
| 导入装载 | `ImportedSymbolLoader.Load`（+ `.Fill` / `.Resolve` 分片） |
| struct 布局 | `StructLayout`（+ `.Query` 读端查询面分片） |
| 派发键 | `DispatchKeys.MangleKey` / `TypeKey` |
| 常量求值 | `ConstEval.Eval` |
| 类级访问 | `TypeRefAccess.CheckTypeRef` / `CheckExposure` |

## 关联文档
- [semantics README](../z42c.semantics/README.md)（Binding / BoundTree / Validation 留在那边）
- [self-hosting](../../../docs/internals/src/compiler/self-hosting.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Symbols/SymbolTable.z42`（4 碎片） | 类名→Z42ClassType / 顶层函数表 + 作用域解析 / FQN 视图 / 起源表 |
| `src/Symbols/SymbolCollector.z42` | 声明收集（类/接口/枚举/函数/属性；各簇 pass） |
| `src/Symbols/ImportedSymbolLoader*.z42` | 跨包 zpkg 符号装载（TSIG/TYPE/SIGS 重建） |
| `src/Types/Z42Type.z42` | 语义类型层次（Prim/Class/Func/Void/Error/Unknown） |
| `src/Types/StructLayout.z42`（+ `.Query`） | struct 字节精确布局（计算/缓存 + 读端查询面） |
| `src/Types/DispatchKeys.z42` | 派发键派生（MangleKey/TypeKey，定义与导入两侧共用） |
| `src/Symbols/ConstEval.z42` | 常量表达式求值（AST → ConstValue） |
| `src/Symbols/TypeRefAccess.z42` | 类级访问强制（静态簇） |
