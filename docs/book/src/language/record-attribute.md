# `[Record]` attribute 与主构造器

> 对齐：2026-08-26（change `add-record-attribute`）

z42 用内建 attribute **`[Record]`** 标记「记录」类型，取代旧的 `record` 关键字。同一根 attribute 可作用于
**`class` 和 `struct`**——把类型的**位置参数**展开为 public 字段 + 主构造器，并在反射里标记 `IsRecord`。
非 `[Record]` 的 `class/struct` 也可带位置参数，那是 **primary constructor（主构造器）**：参数变成
private 字段。

> **为什么从关键字改成 attribute**：C# 的 `record` 既能是 `class` 又能是 `struct`，作为独立关键字与
> class/struct 两条身份轴纠缠。z42 里「自动数据载体」是一根正交、opt-in 的轴——用 attribute 表达，
> 作用于两种身份，语言机制更简单。z42 的 `record` 从来只是「位置参数糖 + 一个反射标记」，**没有** C#
> 的值相等 / `with` / 解构（那些是独立特性，见文末 Deferred）。

## 语法

```z42
[Record] class  Point(int X, int Y);              // public 字段 X/Y + 主构造器 + IsRecord
[Record] struct Vec(int X, int Y);                // 值类型 + IsRecord（bit2 + bit3）
[Record] class  Person(string Name) : Base {      // 位置参数 + 基类 + 块成员并存
    string Greet() => $"Hi {this.Name}";
}

class Counter(int start, int step) {              // 无 [Record] = 主构造器：private 字段
    int cur = 0;
    int Next() { this.cur = this.cur + step; return start + this.cur; }   // 裸 start/step = 私有字段
}
```

- `[Record]` 是保留 directive 名，**豁免 D8 `Attribute` 后缀**（写 `[Record]`，不写 `[RecordAttribute]`）。
- 位置参数 `(params)` 是 class/struct 通用语法位置；`interface` 不接受。

## 语义

| 写法 | 字段可见性 | 主构造器 | `IsRecord`（反射） | 默认基类 |
|------|-----------|---------|-------------------|---------|
| `[Record] class Foo(p...)`  | **public**  | ✓ | **true** | `Std.Object` |
| `[Record] struct Foo(p...)` | **public**  | ✓ | **true** | 无（同 struct） |
| `class Foo(p...)`（无 attr） | **private** | ✓ | false | `Std.Object` |
| `struct Foo(p...)`（无 attr）| **private** | ✓ | false | 无 |

- **基类随底层 Kind 归位**：`[Record] class` 是 `class`，拿到默认 `Std.Object` 基类；`[Record] struct`
  是 `struct`，无默认基类。attribute 不改身份 / 基类轴，只补「位置参数糖 + `IsRecord` 标记」。
- **裸字段访问**：主构造器参数变成同名字段后，方法体里裸写 `start`（不必 `this.start`）即可——z42 把
  类的全部字段播种进方法体作用域，裸标识符命中字段名时按 `this.field` 发码。
- **本次不含值语义**：`[Record]` **不**生成 `Equals` / `GetHashCode` / `ToString` / `==`。它只是旧
  `record` 行为的 attribute 化。值相等是独立特性（Deferred）。

## 实现原理

`[Record]` / 主构造器是**parser 就地脱糖**，无需改名字解析 / binder。数据流：

```
源码  [Record] class Point(int X, int Y) { ... }
  │
  │ Parser 顶层：先解析 attrs=[Record]，把它作参传进 _parseTypeDecl(mods, kind, attrs)
  │ DeclParser._parseTypeDecl（就地展开）：
  │   · class/struct 名后遇 `(` → 解析位置参数
  │   · _attrsHaveRecord(attrs) 判 [Record] → 决定字段可见性
  │   · 位置参数 → FieldDecl（[Record]=public / 否则=private）+ 主构造器 MethodDecl（this.X=X）
  │   · 置于块成员之前；`;` 短形式（无块体）亦可
  ▼
AttributedDecl([Record], ClassDecl{ Kind="class"/"struct", Members=[X,Y,ctor,...块成员] })
  │
  ▼ SymbolCollector → TypeChecker → IrGen → ClassDescBuilder → zbc / 反射
    · 走 Kind 对应机制（record 不再是独立 Kind）
    · IrGen 从原始 AttributedDecl.Attrs 判 [Record]（HandlerRegistry.HasRecord），传
      ClassDescBuilder._classDesc(c, hasRecord) → bit3 → 运行时 __type_is_record
```

**为什么 parser 就地展开、bit3 走 attr 而非新字段**：这是**自举硬约束**。z42c 自建时，`z42c.semantics`
是对着**上一个 nightly 的 `z42c.syntax`**（种子）编译的——若给 `ClassDecl` 加新字段（如 `PrimaryParams`/
`IsRecord`）并从 semantics 读，种子 syntax 没有该字段 → `E0401 no field`（须两-nightly 预种）。把位置参数
展开收进 parser（syntax 内自足）、bit3 由 semantics 读**既有的** `AttributedDecl.Attrs`，即可单-PR 落地。
parser 能判 `[Record]` 是因为把已解析的 `attrs` 作参传进了 `_parseTypeDecl`（顶层 + 嵌套两处调用点）。

**为什么裸字段访问免 binder 改动**：`DeclBinder` 绑定方法体时把类的**全部字段**（含继承）播种进
`TypeEnv`；`ExprTyper._bindIdent` 经 `env.LookupVar` 命中字段名；`AccessEmitter._lookupIdent` 把裸
`BoundIdent`（在字段集里）发成 `FieldGetInstr(this, name)`。所以主构造器参数脱糖成同名字段后，方法体
里裸 `start` 天然解析成 `this.start` 字段读——无需让参数名进名字解析。

**`IsRecord` 与 zbc flags bit3**：`[Record]` 不序列化成 store-meta 反射 blob，而是**复用既有 bit3**——
`ClassDescBuilder` 把 `ClassDecl.IsRecord` 打进类形状 flags 的 bit3（`CLASS_FLAG_RECORD`），运行时
反射 `Type.IsRecord`（`__type_is_record`）读该位。bit3 早已在 zbc 格式里，**零格式-bump**。

## Deferred（独立特性，本 change 不含）

- **值语义**：`Equals` / `GetHashCode` / `ToString` / `==` 值相等生成（C# record 真语义）。
- **主构造器 capture 优化**：C# 12 只在参数被构造后使用时才建 backing field；z42 本版总为每个参数建
  一个 private 字段（功能完整，省一个内存优化）。
- **`with` 非破坏性拷贝 / `Deconstruct` 解构 / init-only**。
