# Spec: `[Record]` attribute

## 能力描述

内建 `[Record]` attribute 标记一个 `class` 或 `struct` 为「记录」——等价于旧 `record` 关键字：把可选的
位置参数 `(params)` 展开为 public 字段 + 主构造器，并在 zbc 类形状 flags 打 `is-record` 位（bit3，供
反射 `__type_is_record` 读）。`record` 关键字被删除。

**本能力 = 纯等价替换**，不含 C# record 的值语义（`Equals`/`==`/`ToString`/`with`/解构均不生成，Deferred）。

## 语法

```
TypeDecl := Attr* Modifier* ('class' | 'struct') Ident TypeParams? PrimaryParams? BaseList? WhereClause? Body
PrimaryParams := '(' (Param (',' Param)*)? ')'
Body := '{' Member* '}' | ';'
```

- `[Record]` 是保留 directive 名，豁免 D8 `Attribute` 后缀（写 `[Record]` 而非 `[RecordAttribute]`）。
- 位置参数 `(params)` 现是 class/struct 通用语法位置（不再是 record 专属）。

## Scenarios

### S1: `[Record] class` 位置参数展开
```z42
[Record] class Point(int X, int Y)
```
- **Then** 等价于 `class Point { public int X; public int Y; Point(int X, int Y) { this.X=X; this.Y=Y; } }`
  且置 `is-record` bit3。
- **And** `Point` 拿到默认 `Std.Object` 基类（Kind=class）。
- **And** 反射 `__type_is_record(typeof(Point))` == `true`。

### S2: `[Record] struct` 位置参数展开
```z42
[Record] struct Vec(int X, int Y)
```
- **Then** 展开为 struct + public 字段 `X`/`Y` + ctor，置 bit2（struct）+ bit3（record）。
- **And** `Vec` 无默认基类（Kind=struct）。
- **And** `__type_is_record` == `true`。

### S3: `[Record]` 带块成员
```z42
[Record] class Money(int Cents) { public string Fmt() { return "$" + this.Cents; } }
```
- **Then** 位置参数字段/ctor 与块成员 `Fmt` 并存。

### S4: `[Record]` 无位置参数
```z42
[Record] class Tag { public string Name; }
```
- **Then** 合法；无合成字段/ctor（无位置参数）；仍置 bit3。

### S5: 用户显式 ctor 与位置参数
> 同旧 record 行为：位置参数合成的 ctor 与用户在块内显式声明的 ctor 是不同重载（不冲突则并存）。
> 本 change 不改此行为（等价替换）。

### S6: 非 `[Record]` 的位置参数 = 主构造器（Decision 3 = A，gate 已确认）
```z42
class Point(int X, int Y) { public int Sum() { return X + Y; } }
```
- **Then** 展开为 `class Point { private int X; private int Y; Point(int X, int Y){ this.X=X; this.Y=Y; } ...}`。
- **And** 类体内裸 `X`/`Y` 解析成私有字段读（经既有「字段入 scope + 裸引用发 `this.X`」机制）。
- **And** **不**置 bit3（`__type_is_record` == `false`）；字段私有（不作为 public 反射面）。
- **And** `Point` 拿到默认 `Std.Object` 基类（Kind=class）。

### S6b: 主构造器参数用于字段初始化器（边界，实现时验证求值序）
```z42
class Box(int Seed) { int val = Seed * 2; }
```
- **Then** `val` 初始化时 `Seed` 解析为 ctor 参数；`this.Seed=Seed` 在字段初始化器求值前完成。

### S7: `record` 关键字已删除（nightly N+1）
```z42
record Point(int X, int Y)
```
- **Then** `record` 不再是关键字 → 解析报 "expected declaration"。

### S8: 等价性 —— 迁移前后行为一致
- **Given** 旧 `record Foo(int A)` 与新 `[Record] class Foo(int A)`
- **Then** 二者产出的类字段布局、ctor、`__type_is_record`、zbc/zpkg 格式**逐字节可比**（Kind 底层
  从 `"record"` 变 `"class"`，但对外可观察行为仅「`[Record] class` 拿回 Object 基」这一处，已在 design
  Decision 4 记录并接受）。

## 非目标（Deferred）

- 值相等 / `GetHashCode` / `ToString` / `==` 值运算符生成 → `add-record-value-semantics`。
- primary constructor 的 capture 优化（仅初始化器用到的参数不成字段）——本 change 总合成私有字段。
- `with` / `Deconstruct` / init-only。
