# z42 对象初始化器 + 字段简写

> **Status**: L1 ✅（add-object-initializers，2026-08-07）
> 声明与初始化简化系列 Change 2；集合字面量 `[]`/`{}` 见 [collection-literals.md](collection-literals.md) / [arrays.md](arrays.md)。

## 设计参考

| 来源 | 借鉴点 |
|------|--------|
| **C#** | 对象初始化器 `new Foo { X = 1, Y = 2 }`（构造后逐字段赋值） |
| **Rust / JavaScript** | 字段简写 `{ x, y }`（同名变量 `x` ≡ `x = x`） |

## 语法

```z42
var p = new Point { X = 1, Y = 2 };        // 对象初始化器：显式字段
var q = new Point { x, y };                // 字段简写：x ≡ x = x（同名局部变量）
var r = new Point { x, Y = 99 };           // 混合
var b = new Box(w, h) { Filled = true };   // 带 ctor 实参
var e = new Point { };                     // 空 ≡ new Point()
```

条目形态（`{ }` 内，逗号分隔，容忍尾逗号）：

- `Ident = expr` → 显式字段初始化；
- 裸 `Ident`（后跟 `,` / `}`）→ 字段简写，等价 `Ident = Ident`（引用同名的在作用域变量）。

## 消歧

`{` 跟在 `new Type`（及可选 `(args)`）之后 = **对象初始化器上下文**，与独立 `{}`（Change 1 的
List/Dict 字面量）、`new T[] { .. }`（数组初始化器，`ty is ArrayType`）天然区分：

| 写法 | 归属 |
|------|------|
| `new Foo { X = 1 }` / `new Foo { x }` | 对象初始化器 |
| `new T[] { 1, 2 }` | 数组初始化器（既有） |
| `{ 1, 2 }` / `{ "a": 1 }`（无 `new`） | List / Dict 字面量（Change 1） |

## 脱糖（纯前端，复用 Change 1 `BoundSeqExpr`，零新 IR / 零格式 bump）

```
new Foo(args) { X = 1, y }
  →  $c = new Foo(args);
     $c.X = 1;
     $c.y = y;      // 简写：值 = 同名变量 y
     ⟨值 = $c⟩
```

字段赋值走既有 `MemberExpr` 赋值绑定 → 自动校验**字段存在性 / 可赋值性 / 可见性**（错字段名 /
只读 / 私有 → 既有诊断）。实现见
[`docs/spec/changes/add-object-initializers/proposal.md`](../../spec/changes/add-object-initializers/proposal.md)。

## Out of Scope（后续）

- **结构更新 `..base`**（Rust struct update `new Foo { ..base, X = 9 }`）：拷贝语义依赖 struct
  真值语义（未 merge），延后到值语义落地再定，避免语义改两次。
- **嵌套对象初始化器** `X = { .. }` / 索引初始化器 `[k] = v`。
- **集合初始化器** `new List<int> { 1, 2 }`：Change 1 的 `{1,2}` 字面量已覆盖建 List。
- 两阶段 nightly 纪律：z42c / stdlib 源码*使用*本语法要晚一个 nightly。
