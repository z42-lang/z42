# 对象初始化器 + 字段简写

> 对齐：2026-08-07（change `add-object-initializers`）

在 `new` 表达式后跟一对花括号，**构造完成后逐字段赋值**：

```z42
var p = new Point { X = 1, Y = 2 };        // 显式字段
var q = new Point { x, y };                // 字段简写：x ≡ x = x（同名的在作用域变量）
var r = new Point { x, Y = 99 };           // 混合
var b = new Box(w, h) { Filled = true };   // 带 ctor 实参
var e = new Point { };                     // 空 ≡ new Point()
```

条目形态（`{ }` 内，逗号分隔，容忍尾逗号）：

- `Ident = expr` → 显式字段初始化；
- 裸 `Ident`（后跟 `,` / `}`）→ 字段简写，等价 `Ident = Ident`。

## 消歧：`{` 什么时候是对象初始化器

`{` 跟在 `new Type`（及可选 `(args)`）之后 = 对象初始化器，与独立的 `{}`
（[List / Dictionary 字面量](collection-literals.md)）、`new T[] { .. }`（数组初始化器）天然区分：

| 写法 | 归属 |
|------|------|
| `new Foo { X = 1 }` / `new Foo { x }` | 对象初始化器 |
| `new T[] { 1, 2 }` | 数组初始化器（见 [数组](arrays.md)） |
| `{ 1, 2 }` / `{ "a": 1 }`（无 `new`） | [List / Dictionary 字面量](collection-literals.md) |

## 语义

等价于「构造 → 逐字段赋值 → 取该对象为表达式的值」：

```
new Foo(args) { X = 1, y }
  →  $c = new Foo(args);
     $c.X = 1;
     $c.y = y;      // 简写：值 = 同名变量 y
     ⟨值 = $c⟩
```

字段赋值走**普通成员赋值**的那套规则，因此**字段存在性 / 可赋值性 / 可见性**照常校验：
错的字段名、只读字段、私有字段各报其既有诊断。

## 尚不支持

- **结构更新 `..base`**（`new Foo { ..base, X = 9 }`）；
- **嵌套对象初始化器** `X = { .. }` 与索引初始化器 `[k] = v`；
- **集合初始化器** `new List<int> { 1, 2 }` —— 用 [`{1, 2}` 字面量](collection-literals.md) 直接建 List。

## 相关

- [集合字面量 `{}`](collection-literals.md) —— List / Dictionary 侧
- [数组](arrays.md) —— `[]` 与 `new T[]{...}` 侧
