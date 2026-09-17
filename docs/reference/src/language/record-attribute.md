# `[Record]` attribute 与主构造器

> 对齐：2026-08-26（change `add-record-attribute` + `add-record-value-semantics`；含值相等 / 记录式 ToString）

z42 用内建 attribute **`[Record]`** 标记「记录」类型，**取代已删除的 `record` 关键字**（`record` 现为普通
标识符，可作类型 / 变量名）。同一根 attribute 可作用于
**`class` 和 `struct`**——把类型的**位置参数**展开为 public 字段 + 主构造器，并在反射里标记 `IsRecord`。
非 `[Record]` 的 `class/struct` 也可带位置参数，那是 **primary constructor（主构造器）**：参数变成
private 字段。

> **为什么从关键字改成 attribute**：C# 的 `record` 既能是 `class` 又能是 `struct`，作为独立关键字与
> class/struct 两条身份轴纠缠。z42 里「自动数据载体」是一根正交、opt-in 的轴——用 attribute 表达，
> 作用于两种身份，语言机制更简单。`[Record]` 提供「位置参数糖 + 反射标记 + **值语义**（值相等 / 记录式
> ToString）」；`with` / 解构 / init-only 仍是独立特性（见文末 Deferred）。

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
- **值语义**（见下节）：`[Record]` 类型自动获得 member-wise **值相等**（`Equals` / `GetHashCode` /
  `==` / `!=`，type-exact）与**记录式 `ToString`**（`T { A = v, B = v }`）。用户显式声明同名成员时合成让位。

## 值语义（值相等 + 记录式 ToString）

`[Record]` 类型（class & struct）自动获得 C# record 的核心值语义。**纯编译期合成**（复用既有 IR 指令），
零 zbc/zpkg 格式-bump。

```z42
[Record] class Point(int X, int Y);

var a = new Point(1, 2);
var b = new Point(1, 2);
a == b            // true（值相等，非句柄身份）
a.Equals(b)       // true
a != new Point(1, 9)   // true
a.GetHashCode() == b.GetHashCode()   // true（等值等哈希）
a.ToString()      // "Point { X = 1, Y = 2 }"
new Empty().ToString()   // "Empty { }"（无字段）
```

| 面 | `[Record] class`（引用类型） | `[Record] struct`（值类型） |
|----|------------------------------|------------------------------|
| `Equals` / `==` / `!=` | 合成 member-wise 值相等（vtable 覆盖 `Std.Object`） | 既有 blob 逐叶子值相等（`==` 内联；boxed `.Equals`） |
| `GetHashCode` | 合成 `h=17; h=h*31+field.GetHashCode()`（& 0x7fffffff） | VM 原生 FNV（blob 值哈希） |
| `ToString` | 合成记录格式（vtable 覆盖） | 合成记录格式（VM 让路原生类型名拦截，见下） |

**规则**：

- **type-exact（对齐 C# EqualityContract）**：相等先比运行时类型（`GetType().FullName`）——`Base(1)` **不等于**
  `Derived(1, 2)` 即使基字段相同；异类型 / `null` / 非 record 对象比较均返 `false`（不抛）。
- **字段范围**：**相等**比 **全部实例字段**（含 private、基类继承字段，声明序基类在前）；**ToString** 只打
  **public 成员**（private 字段参与相等但不出现在 ToString——与 C# PrintMembers 一致）。
- **引用字段递归**：引用类型字段用其自身 `Equals` / `ToString` 递归值比较 / 格式化（null-safe）。
- **用户优先**：显式声明 `Equals` / `GetHashCode` / `ToString` 时，对应合成让位、不覆盖。

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

**值语义合成（`RecordSynth.z42`）**：`IrGen` 的合成循环对 `HandlerRegistry.HasRecord` 为真的类型，调
`RecordSynthEmitter` 直接搭 `EmitContext` 发 IR（无 SemanticModel body，镜像既有 struct
`EmitSynthStructEquals`）。产物按 class/struct 分流：

```
[Record] class（引用类型）→ 合成 <FQ>.Equals / <FQ>.GetHashCode / <FQ>.ToString（用户未声明才合成）
  · VM 的 own_methods 扫描（type_registry：按 <class>. 前缀收函数，derive_simple_method_name 剥 $N）
    自动把它们纳入 vtable → 覆盖 Std.Object 的同名槽。**方法名用裸名**（`Equals` 非 `Equals$1`）。
  · Equals 骨架：other==null→false；GetType().FullName==<FQ> 门（type-exact）；逐字段（基元 eq /
    引用 null-safe .Equals）短路合取。字段**直接从 other 读**（field_get），不经 as_cast 中转。
  · ==/!=：OperatorEmitter 拦截 record-class 操作数 → 发 null-safe 值 Equals 调用（镜像 blob-struct ==）。

[Record] struct（值类型）→ 只合成 <FQ>.ToString（相等=既有 blob 值相等 / 哈希=native FNV，不动）
  · VM 对 boxed-struct 零参 ToString **无条件原生拦截**返回类型名（interp exec_vcall + jit vcall）；
    加 `!type_desc.is_record()` 守卫 → record struct 落到候选查找命中合成 ToString。**格式化逻辑单一
    真相源在编译器**（class/struct 共用），VM 只让路。`is_record()` 读 CLASS_FLAG_RECORD（bit3）。
```

> **两个 jit 专属铁律（interp 容忍、jit 会错，调试代价高）**：① 合成的 **class** 方法用**裸名**——`$N`
> arity 后缀致 jit vtable 派发错配（命中 Object 身份版 → 值相等失效）；struct 用 `Equals$1` 是因 boxed-vcall
> 候选查找路径不同。② **`as_cast` 后 `field_get` 读结果 jit 会误编**（引用类型）——type-exact 已确认类型，
> 直接 `field_get other.field`（镜像普通 codegen）。合成 IR 必须 interp+jit 双验。

## Deferred（独立特性，本 change 不含）

- **`with` 非破坏性拷贝**（`r with { X = 5 }`）——需新语法/关键字，走两-nightly support-先行纪律。
- **`Deconstruct` 解构**（`var (x, y) = p`）——需 tuple 支持。
- **`init`-only setters**。
- **主构造器 capture 优化**：C# 12 只在参数被构造后使用时才建 backing field；z42 本版总为每个参数建
  一个 private 字段（功能完整，省一个内存优化）。
- **struct record 嵌套值-struct 字段的 ToString**：本版 struct 记录式 ToString 覆盖扁平叶子字段
  （基元 / string / 引用）；嵌套 value-struct 字段的递归格式化留待后续。
