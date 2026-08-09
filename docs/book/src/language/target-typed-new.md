# target-typed `new`（省略构造类名）

> 对齐：2026-08-09（change `add-target-typed-new`）

当**目标类型已知**时，`new(args)` 可省略构造的类名，由编译器从目标类型推断——镜像 C# 9 的
target-typed `new`。构造对象时不必把类名写两遍：

```z42
Dictionary<string, List<int>> m = new();          // 等价 new Dictionary<string, List<int>>()
Route r = new("/users", "POST");                    // 等价 new Route("/users", "POST")
```

**纯语法糖**：语义层用目标类型替换省略的类名，产出的 IR（`obj_new`）与显式 `new T(args)` **逐字节
相同**，无运行时 / 无 IR / 无 zbc·zpkg 格式变化。

## 语法

判据：**`new` 紧跟 `(`** → target-typed（省略类名）。可选跟对象初始化器 `{ ... }`：

```z42
A a = new();                       // 无参
A a = new(1, 2);                   // 带构造实参
A a = new() { X = 1, Y = 2 };      // + 对象初始化器
```

`new T(...)` / `new T[...]` / `new T { ... }`（显式写出类名）路径完全不变。

## 目标类型的来源（5 个位置）

编译器在下列位置为 `new()` 提供目标类型：

| 位置 | 目标类型 | 示例 |
|------|---------|------|
| 局部变量声明 | 声明类型 | `A a = new();` |
| return 语句 | 函数返回类型 | `A Make() { return new(); }` |
| 赋值 | 左值类型 | `a = new();` |
| 字段初始化器（实例 / 静态） | 字段类型 | `class C { A f = new(); }` |
| 调用实参 | 形参类型 | `Take(new());` |

## 强制规则（编译期诊断）

| 情形 | 诊断 |
|------|------|
| 目标类型不可推断（如 `var a = new();` 双向都推不出） | `E0437` |
| 重载调用中 `new()` 实参无法定型（≥2 个同 arity 重载靠类型消歧） | `E0437` |

```z42
var x = new();              // ✗ E0437：var 无法为 target-typed new 提供类型
// 有 void F(A), void F(B) 两个重载时：
this.F(new());              // ✗ E0437：重载歧义，需写 new A() / new B()
```

无歧义的重载（仅一个候选匹配 arity）中，`new()` 实参按选中重载的形参类型定型，**不报错**。

## 实现原理

前端脱糖，落在语义绑定阶段：

1. **解析**：`new(` 前瞻一个 token 即判定 target-typed（类名不可能以 `(` 起头，函数类型
   `(T)->R` 不可被 `new`），产出 `Type == null` 的 `ObjNewExpr` / `ObjInitExpr`。
2. **绑定**：`_bindNew(n, env, expected)` 在 `Type == null` 时用 `expected`（目标类型）替代
   `ResolveType(n.Type)`，其余 ctor 解析 / arity 校验 / 命名实参适配逻辑不变。`BindWithTarget`
   是目标类型已知位置的统一入口。
3. **传参的先有蛋问题**：调用管线先绑实参再做重载决议，而 `new()` 定型需要形参类型（=重载结果）。
   解法是**延迟绑定 + 决议容忍 + 回填**：target-typed new 实参先留占位；重载决议在 arity 唯一时
   无需类型即可选中（歧义则报 `E0437`）；选定后按形参类型回填。命名实参 / 默认值路径的
   `_adaptArgs` 本就持有形参类型，就地定型。

详见 change 提案与设计：`docs/spec/archive/…-add-target-typed-new/`。

## 当前边界（Deferred）

- **集合字面量元素位**：`A[] a = { new(), new() }` 中元素不接收目标类型 → `E0437`。
- **三元 / switch 分支**：`A a = c ? new() : new()` 分支无 expected 传播 → `E0437`。
- **默认参数值**：`void f(A a = new())` 不覆盖。

以上均**明确报错、不误编**；需要时写出显式类名 `new A()`。

## 关联文档

- 相关简化：[集合字面量](../../../design/language/collection-literals.md) / [数组字面量](../../../design/language/arrays.md)
- 引入：change `add-target-typed-new`（`docs/spec/archive/`）
