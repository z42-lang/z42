# Proposal: 对象初始化器 + 字段简写（声明与初始化简化 Change 2）

> Status: **IMPL 中**（User 确认范围 2026-08-07：对象初始化器 + 字段简写；`..base` 延后）
> 分类：lang（新语法）→ 规范先行；子系统：compiler（纯前端）
> 承接 [add-collection-literals](../add-collection-literals/proposal.md)（Change 1），复用其 `BoundSeqExpr`。

## Why

Change 1 做了集合的字面量化；Change 2 做**对象**的初始化简化。当前建对象后设字段只能逐句：

```z42
var a = new Account();
a.owner = "alice";
a.balance = 100;        // 连续字段赋值噪音（见 examples/partial.z42）
```

引入 C# 对象初始化器 + Rust/JS 字段简写：

```z42
var a = new Account { owner = "alice", balance = 100 };   // 对象初始化器（C#）
var p = new Point { x, y };                               // 字段简写（Rust/JS）：x→x=x
var b = new Box(w, h) { Filled = true };                  // 带 ctor 实参
```

## What Changes

- **对象初始化器** `new Type(args?) { F1 = v1, F2 = v2 }`：构造后逐字段赋值。
- **字段简写** `new Type { x, y }`：裸标识符 `x` ≡ `x = x`（同名局部变量），借鉴 Rust struct
  literal / JS 对象简写。
- **脱糖（复用 Change 1 `BoundSeqExpr`，零新 Bound 节点、零 IR、零格式 bump）**：
  `new Foo(args) { X = 1, y }` → `$c = new Foo(args); $c.X = 1; $c.y = y; ⟨$c⟩`。
  字段赋值走既有 `MemberExpr` 赋值绑定 → 自动校验字段存在性 / 可赋值性 / 可见性。

## 消歧

`{` 跟在 `new Type`（及可选 `(args)`）之后 = 对象初始化器上下文，与独立 `{}`（Change 1 的
List/Dict 字面量）天然区分。对象初始化器内条目：

- `Ident = expr` → 显式字段初始化；
- 裸 `Ident`（后跟 `,` 或 `}`）→ 字段简写（`Ident = Ident`）；
- `new T[] { .. }`（`ty is ArrayType`）仍走既有数组初始化器，不受影响。

## Scope（改动文件）

| 文件 | 改动 |
|------|------|
| `z42c.syntax/src/Ast.z42` | +`ObjInitExpr`（Type + ctor Args + 字段名/值数组） |
| `z42c.syntax/src/ExprParser.z42` | `new` 块重构：array-init 加 `ty is ArrayType` 卫；解析可选 `(args)` 后 `{` → 对象初始化器 |
| `z42c.semantics/src/ExprTyper.z42` | `_bindObjInit`：合成 `new` + 逐字段 `MemberExpr` 赋值 → `BoundSeqExpr`；`_bindExpr` 分派 |
| `docs/design/language/` | 对象初始化器语法节（object-initializers.md 或并入 language-overview） |
| `examples/object_initializers.z42` | NEW 示例 |
| `src/tests/basic/object_initializers.z42` | NEW golden（Assert 自校验） |

## Out of Scope

- **结构更新 `..base`**（Rust struct update）：拷贝语义依赖 struct 真值语义（未 merge，见
  struct-value-semantics-program），延后到值语义落地，避免定义两次语义。
- **集合初始化器** `new List<int> { 1, 2, 3 }`（bare 表达式条目）：Change 1 的 `{1,2,3}` 字面量已
  覆盖建 List 的需求；本轮对象初始化器条目要求 `Ident`/`Ident=expr`，非 Ident 报错。
- **嵌套对象初始化器** `X = { .. }` / 索引初始化器 `[k] = v`：留后续。
- z42c/stdlib 源码*使用*本语法：两阶段 nightly 纪律，晚一个 nightly 的 follow-up。
