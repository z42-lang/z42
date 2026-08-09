# Proposal: target-typed `new`（`A a = new()` 省略构造类名）

> Status: **DRAFT → IMPL**（2026-08-09；User 已确认全位置含传参重构）
> 分类：lang（新语法）→ 走规范先行流程
> 子系统：compiler（纯前端）
> 实现原理与用户文档见 [book: target-typed new](../../../book/src/language/target-typed-new.md)

## Why

构造对象时右侧的类名与左侧声明类型重复，是高频噪音：

```z42
Dictionary<string, List<int>> m = new Dictionary<string, List<int>>();   // 类名写两遍
Route r = new Route("/u", "POST");                                        // Route 重复
```

对照 C# 9 的 target-typed `new`，只要目标类型已知，右侧构造可省略类名：

```z42
Dictionary<string, List<int>> m = new();          // 从声明类型推断
Route r = new("/u", "POST");
```

**关键点：纯前端 desugar，零运行时 / 零 IR / 零格式 bump。** `new(args)` 在语义层用**目标类型**
（左侧声明类型 / 返回类型 / 赋值 LHS 类型 / 形参类型）替代省略的类名，产出的 `BoundNew` 与显式
`new T(args)` 逐字节相同。因此**不需要 zbc/zpkg minor bump**，唯一约束是
[两阶段 nightly 纪律](../../../.claude/rules/bootstrap-seed.md)的**语法轴**（support 先行、晚一个
nightly 才能在 z42c/stdlib 源码里 use）。

## 语法

```z42
A a = new();                      // 目标 = 声明类型 A
A a = new(1, 2);                  // 带构造实参
A a = new() { X = 1, Y = 2 };     // target-typed + 对象初始化器
a = new();                        // 赋值：目标 = LHS 类型
A Make() { return new(); }        // return：目标 = 返回类型 A
class C { A field = new(); }       // 字段：目标 = 字段类型
f(new());                          // 传参：目标 = 形参类型
```

判据：**`new` 紧跟 `(`** → target-typed（省略类名）。`new T(...)` / `new T[...]` / `new T{...}`
（显式类名）路径完全不变。

## What Changes

### 目标类型的 5 个来源（全位置）

| 位置 | 目标类型来源 | 站点 |
|------|------------|------|
| 局部变量 `A a = new()` | 声明类型 `ResolveType(v.Type)` | `StmtBinder._bindVarDecl` |
| return `return new()` | 函数返回类型（`ret` 形参） | `StmtBinder._bindReturn` |
| 赋值 `a = new()` | 已绑定 LHS 的 `Type()` | `ExprTyper._bindAssign` |
| 实例字段 `A f = new()` | 字段类型（经 `this.f = init` assign desugar，复用赋值路径） | `DeclBinder`（自动覆盖）|
| 静态字段 `static A f = new()` | 字段类型 `fd.Type` | `DeclBinder`（`AddStaticInit`）|
| 传参 `f(new())` | 重载选定后的形参类型 | 调用管线（见 D3）|

### 核心机制

- `ObjNewExpr.Type` / `ObjInitExpr.Type` 允许为 `null`（= target-typed 标记）。
- `_bindNew(n, env, expected)` / `_bindObjInit(oi, env, expected)` 新增 `expected` 目标类型入参；
  `Type == null` 时用 `expected` 替代 `env.ResolveType(n.Type)`，其余逻辑（ctor 解析、arity 校验、
  named-arg 适配）完全不变。
- `ExprTyper.BindWithTarget(e, target, env)`：目标类型已知的统一入口——target-typed new/obj-init
  时走带 `expected` 的绑定，否则回落普通 `_bindExpr`。5 个站点均改调它。

## 核心设计决策

### D1. target-typed 检测 = `new` 紧跟 `(`

`new` 之后唯一以 `(` 开头的合法构造是 target-typed（`new T()` 的类名不可能以 `(` 起头；函数类型
`(T)->R` 无法被 `new`）。故前瞻一个 token 即可无歧义判定，不影响任何现有 `new T(...)` 解析。

### D2. `expected` 必须是可实例化的具体类型

`expected` 为 `null` / `Unknown` / `Error`（如 `var a = new()` 双向都推不出）→ 报错
**E04xx TargetTypedNewNeedsType**「target-typed `new` 需要已知的目标类型」。`expected` 是接口/抽象
类时沿用 `_bindNew` 既有的 ctor 解析报错路径（无可用 ctor）。

### D3. 传参位置：延迟绑定 + 重载决议容忍未知类型位（crux）

调用管线现状是**先把所有实参绑成 `BoundExpr`，再做重载决议**（`_resolveOverload` 读
`args[k].Type()`）。target-typed new 绑定时就需目标类型（=形参类型），而形参类型要等重载选定后才知道
→ 先有蛋悖论。解法：

1. **延迟**：实参绑定阶段，target-typed new 位留 `null` 占位（`_bindCall` / `_bindNew` ctor args）。
2. **容忍**：`_resolveOverload` 遇 `null` 位——arity 唯一（`na==1`）→ 直接选中（不需类型）；
   arity ≥2 需按类型决议但存在 `null` 位 → 报错**「target-typed new 实参在重载调用中需显式类型」**。
3. **回填**：重载/签名选定后，用形参类型 `_fillDeferredArgs` 把 `null` 位按 `sig.ParamTypes[i]` 绑定。
   汇聚点 `_withDefaults` 覆盖 free/static/prim-static/instance 四形态；其余形态
   （same-class static / 局部函数 / Func 值 indirect / ns-free）在选定 sig 处显式回填。
   named-arg / 默认值路径经 `_adaptArgs`——它本就持 `md.Params[i].Type`，就地按形参类型绑定。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | `ObjNewExpr.Type` / `ObjInitExpr.Type` 允许 null + `Dump` 空守卫 |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | `new(` 前瞻 → target-typed `ObjNewExpr(null,..)` / `ObjInitExpr(null,..)` |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindNew`/`_bindObjInit` 加 `expected`；`BindWithTarget`/`IsTargetTypedNew`；`_bindAssign` RHS 目标类型；ctor args 延迟 |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | `_bindVarDecl` / `_bindReturn` 接目标类型 |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 静态字段 init 接字段类型（实例字段经 assign desugar 自动覆盖）|
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | 调用管线实参延迟 + 各形态回填 |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | `_resolveOverload` 容忍 null 位；`_withDefaults` 回填；`_adaptArgs` 按形参类型绑 |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新增 `TargetTypedNewNeedsType` 诊断码 |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | target-typed new 与显式 new 逐字节等价 |
| `src/compiler/z42c.semantics/tests/typecheck/typecheck_tests.z42` | MODIFY | 无目标 / 重载歧义 报错用例 |
| `src/compiler/z42c.syntax/tests/parser/parser_tests.z42` | MODIFY | `new(` 解析出 target-typed AST |
| `docs/book/` 对应机制页 | MODIFY | target-typed new 语法 + desugar 原理 |
| `examples/target_typed_new.z42` | NEW | 示例 |

## Out of Scope（明确延后）

- **集合字面量元素位 `A[] a = { new(), new() }`**：coll-lit 元素当前无目标类型下传（`_bindArrayLit`
  按 `_bindExpr` 绑元素）→ 报错。需 coll-lit 元素级 target 传播，另做。
- **三元 / switch 分支 `A a = c ? new() : new()`**：分支无 expected 传播 → 报错。
- **默认参数值 `void f(A a = new())`**：默认值表达式罕见用 new()，本轮不覆盖。
- **z42c / stdlib / xtask 源码使用 `new()`**：两阶段纪律，晚一个 nightly 的 follow-up。本轮只落"支持"。
- JIT / AOT：纯前端 lowering，VM 路径不变。
