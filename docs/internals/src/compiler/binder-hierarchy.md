# Binder Hierarchy

> **页型**: 机制页 ｜ **代码**: `src/compiler/z42c.semantics/src/`
> **相关**: [源代码编译流程](source-compile.md)
> **待办**: Binder 链尚未实现，TypeChecker 当前走 `TypeEnv`

> Roslyn-style polymorphic Binder chain replacing the monolithic `TypeEnv`：
> 本页记录长期设计与分阶段迁移计划。

## 设计目标

把 TypeChecker 当前对 `TypeEnv` 的依赖 —— 一个 sealed class with ~10
dictionaries 持各种 scope 信息 —— 重构为多态 `Binder` 子类链。每种 scope
（global / method / block / class / lambda / catch / pinned / ...）对应
一个 Binder 子类，沿 `Next` 链 forward lookup。

## 为什么不继续用 TypeEnv

当前 `TypeEnv`（`src/compiler/z42c.semantics/src/Types/TypeEnv.z42`）
聚合了所有 scope 的所有 slot 类型：

- 父链：`Parent`
- 本层 scope：`Vars` / `Consts` / `LocalFns`
- 全局：`Symbols`（`SymbolTable`）
- 类上下文：`ClassName` / `TypeParamNames` / `InCtorThis` / `InStaticCtor`
- 其余按需追加的 scope 状态字段（`FnLocals` 等）+ 一组 `Lookup*` 方法

每个 TypeEnv 实例都分配所有这些字典，即便它只 represent 一个简单 `if`
块（只关心 `_vars`，其余字段是浪费的内存 + cache pressure）。加新 scope
关心点（e.g. `pinned` 块的 pinned-locals）要扩 TypeEnv 字段，破坏了"每个
scope 关心自己"的封装。

更深的问题：lookup 路径是 hardcoded 在 TypeEnv 各 method 里
（`LookupVar` 先查 `_vars` 再走 parent；`LookupFunc` 先查 `_localFuncs`
再走 parent 再查 global `_funcs`）。换种 scope 行为（e.g. `using` 别名
解析）要么往 TypeEnv 加新 method，要么 caller 写嵌套 if-else 围绕 TypeEnv。

## Roslyn 对照

Roslyn 的 `Binder` (`/src/Compilers/CSharp/Portable/Binder/Binder.cs`)
是 ~3000 LOC abstract class，30+ 子类：

```
Binder
├── BuckStopsHereBinder         (root)
├── InContainerBinder           (namespace / class container)
│   ├── NamespaceOrTypeAndAliasMapBinder  (`using` directive)
│   ├── WithExternAndUsingAliasesBinder
│   └── ...
├── InMethodBinder              (method body)
│   ├── InLocalFunctionBinder
│   └── InAsyncMethodBinder
├── BlockBinder                 ({ ... } scope)
│   ├── ForLoopBinder           (`for (var i = 0; ...)`)
│   ├── ForEachLoopBinder
│   ├── UsingStatementBinder
│   └── ...
├── CatchClauseBinder           (`catch (T e)` exception variable scope)
├── LambdaBinder                (lambda param scope)
├── SwitchSectionBinder         (switch arm pattern variables)
└── ... ~20 more
```

每个子类只 own 自己 scope 的 slot 类型。Lookup 默认 forward to `Next`，
具体子类 override 来在 forward 前先查自己。

## 现状

Binder 链是设计目标，代码中尚无 `Binder` 基类或子类；`TypeChecker`（`src/compiler/z42c.semantics/src/Binding/TypeChecker.z42`）与各 `*Typer` / `*Binder` 统一经 `TypeEnv` 查 scope。

拟引入的子类与 `TypeEnv` 概念的对应：

| 类 | 对应 TypeEnv 概念 |
|---|---|
| `Binder` | abstract base |
| `GlobalScopeBinder` | 全局符号表 |
| `InMethodBinder` | 参数修饰符 + `ClassName` |
| `InBlockBinder` | `Vars` |

## Phase 2-N Migration Plan

### Phase 2: 第一个真实 consumer — `BindClassMethods`

候选第一个迁移点是 `TypeChecker.TryBindClassMethods`：

```z42
void TryBindClassMethods(ClassDecl cls) {
    var classEnv = _rootEnv.WithClass(cls.Name);     // current TypeEnv path
    foreach (var method in cls.Methods) {
        var scope = classEnv.PushScope();
        // ... add params ...
        // ... bind body ...
    }
}
```

Phase 2 重写为：

```z42
void TryBindClassMethods(ClassDecl cls) {
    var classBinder = new InClassBinder(_globalBinder, cls.Symbol);  // NEW: InClassBinder Phase 2
    foreach (var method in cls.Methods) {
        var methodBinder = new InMethodBinder(classBinder);
        foreach (var p in method.Params) methodBinder.DefineParameter(p.Symbol);
        // ... bind body using methodBinder.LookupSymbol() instead of scope.LookupVar() ...
    }
}
```

需要：
- NEW `InClassBinder` (Phase 2 specific class scope binder)
- `ExprTyper` 的标识符绑定 改走 `_currentBinder.LookupSymbol(name)`
  替代 `env.LookupVar(name)` / `env.IsClassName(name)`
- TypeEnv 当前其它 caller (closure / for / try) 仍保留 TypeEnv 走原路径 —
  gradual migration

### Phase 3: Block-level scopes

迁移所有 `BindBlock` / `BindIf` / `BindFor` / `BindForeach` 到
`InBlockBinder` + 各自专用子类（`ForLoopBinder` etc.）。

### Phase 4: Lambda + catch + pinned

`InLambdaBinder` (capture context), `InCatchBinder` (catch variable
scope), `InPinnedBinder` (`pinned p = s { ... }` block).

### Phase 5: TypeEnv 退役

所有 TypeChecker binding sites 都走 Binder 之后，删除 TypeEnv。

## 不变量（必须遵守）

每个 Binder 子类 override `LookupSymbol`：

```z42
public override ISymbol LookupSymbol(string name) {
    if (_mySlot.TryGetValue(name, out var s)) return s;
    return base.LookupSymbol(name);  // ← 必须 forward；否则破坏 shadowing 语义
}
```

漏掉 `base.LookupSymbol(name)` 会让"内层没找到但外层有"的合法 lookup 失败。

## 与 ISymbol (F2.2) 的关系

Binder.LookupSymbol 返回 `ISymbol?`（review.md F2.2 Phase 1 的产物）。
Binder 接入 TypeChecker 后，会 return TypeChecker 已构造的 MethodSymbol /
FieldSymbol 实例 + 后续引入的 LocalSymbol / ParameterSymbol。

## Deferred / Future Work

- **ImportAliases 子类** —— `using Foo = Bar;` 别名解析；当 Phase 3 落地
  `using Foo = Bar;` 语法时引入。
- **GenericConstraintBinder** —— generic 类型参数约束查找（current
  TypeEnv 没有；scope 加 type params）。
- **接 LSP 协议** —— Binder.LookupSymbol 是天然的 "GoToDefinition" 实现
  入口（先 Binder.LookupSymbol(name) → ISymbol.DeclarationSpan）。
