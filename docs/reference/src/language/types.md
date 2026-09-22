# 基本类型与字面量

z42 的内建标量类型**每种只有一套拼写**：C# 风格的关键字（`int`、`byte`、`double`…），外加等价的
BCL 包装类型名（`Int32`、`Byte`、`Double`…），二者关系同 C# 的 `int` ⟷ `System.Int32`。

## 内建类型总表

| 关键字 | 包装类型 | 位宽 | 说明 |
|--------|----------|------|------|
| `sbyte` | `Std.SByte` | 8 | 有符号 |
| `short` | `Std.Int16` | 16 | 有符号 |
| `int` | `Std.Int32` | 32 | 有符号 |
| `long` | `Std.Int64` | 64 | 有符号 |
| `byte` | `Std.Byte` | 8 | 无符号 |
| `ushort` | `Std.UInt16` | 16 | 无符号 |
| `uint` | `Std.UInt32` | 32 | 无符号 |
| `ulong` | `Std.UInt64` | 64 | 无符号 |
| `float` | `Std.Single` | 32 | IEEE-754 |
| `double` | `Std.Double` | 64 | IEEE-754 |
| `bool` | `Std.Boolean` | — | `true` / `false` |
| `char` | `Std.Char` | 32 | 一个 Unicode 标量值 |
| `string` | `Std.String` | — | 不可变 UTF-8，引用类型 |
| `object` | `Std.Object` | — | 所有引用类型的基类，装箱目标 |
| `void` | — | — | 仅用于返回类型 |

**关键字就是规范形式**：编译器把包装名 / FQ 名（`Int32` / `Std.Int32`）归一到关键字后再比较类型，
写进 zbc / zpkg 的类型名也是这一套。因此

```z42
void F(int x)    { }
void F(Int32 x)  { }   // ✗ 重复声明——归一后是同一个 F(int)
```

`x.GetType().Name` 返回包装类型名（`Int32`），`FullName` 是 `Std.Int32`。

### 没有 Rust 风格短名（drop-short-primitive-aliases，2026-09-22）

`i8` / `i16` / `i32` / `i64` / `u8` / `u16` / `u32` / `u64` / `f32` / `f64` **不是 z42 的类型拼写**。
它们曾作为关键字 token 与关键字并存，现已从词法器移除；在源码里它们只是普通标识符，用作类型即报
「未定义类型」。

删除理由：同一个类型有两个名字，既让重载 / 诊断 / 反射的「显示哪个名字」处处需要抉择，也让
canonical 表里 `int`/`long`/`float`/`double` 走关键字、窄整数族走短名，长出两套风格。收敛后
**源码拼写 == canonical == zbc 线格式名**，只剩一张表。

短名仍存在于两个**与 z42 类型拼写无关**的记法里，两者都刻意保留：

| 记法 | 例子 | 说明 |
|---|---|---|
| `[Extern]` FFI 签名串 | `u8`、`usize`、`*const T`、`CStr` | C / Rust ABI 记法，由 VM 的 `native/dispatch.rs` 解析（见 interop 参考页） |
| IR 文本 dump | `add i32 %1, %2` | LLVM 风格的 IR 汇编记法 |

这与 C# 的「源码 `int` / CIL `int32` / 元数据 `System.Int32`」分层同形：同一个类型在不同层用不同
记法是正常的，**同一层里有两个名字**才是要消除的冗余。

> **z42 没有的**：`decimal`、`nint` / `nuint`、`System.*` 的任何类型。

## 字面量

```z42
int    x    = 42;
int    hex  = 0xFF;
long   big  = 9_000_000_000L;   // `_` 数字分隔符；`L` 后缀 = long
double pi   = 3.14159;          // 无后缀的小数 = double
float  f    = 1.5f;             // `f` 后缀 = float
bool   flag = true;
char   ch   = 'z';
string s    = "hello";
object o    = null;
```

字符串与字符字面量的转义规则见[字符串](strings.md)。

## `var` 类型推断

`var` 声明一个类型由**初始化器**推断的变量，取初始化器的静态类型：

```z42
var count = 0;      // int
var name  = "z42";  // string
var ratio = 0.5;    // double
```

`var` 的正规用法是带初始化器的局部变量。不带初始化器的 `var x;` 当前不会被编译器拒绝，
但类型无从推断，不要依赖这种写法。

## 可空标记 `?` 与值类型

**值类型永不为 null。** `int` / `bool` / `char` / 浮点 / `enum` / `struct` 的槽位里不可能
出现 null，编译器在三个入口把它挡住：

```z42
int x = null;        // ✗ E0475：值类型不接受 null
if (n == null) { }   // ✗ E0474：值类型与 null 比较恒假 —— 静默恒假是最坏的形态
int? a = 42;         // ✗ E0476：值类型不允许 `?`
```

第二条单列是有理由的：静默恒假意味着**代码看起来做了检查、那个分支实际从不进入**，
比直接报错难查得多。

`?` 只用于**引用类型**：

```z42
string?    maybeNull = null;    // ✓
IPAddress? addr      = null;    // ✓
byte[]?    buf       = null;    // ✓ —— 数组是引用类型，可空的是数组本身不是元素
```

### 引用类型的 `?` 是一个**请求**，不是一个描述

引用类型**默认可空**，`?` 不改变这一点——`string?` 与 `string` 仍是同一个类型，
赋 `null` 给两者都合法。`?` 表达的是另一件事：

> **「请编译器在这里强制检查空值。」**

所以**不标 `?` 就完全不受检**。这是刻意的：存量代码一行不用改，也不会出现 C# 里
「一处改了整条调用链都要跟着改」的传染。想在某个接口上严格起来，就只在那里标。

目前生效的有两处：标了 `?` 的**形参**，和标了 `?` 的**返回值**。解引用之前必须先检查，
否则报 **E0478**。

```z42
int Length(string? s) {
    return s.Length;              // ✗ E0478：标了 `?`，解引用前必须检查
}

int Length(string s) {
    return s.Length;              // ✓ 没标 `?` ⇒ 不受检（存量代码的默认状态）
}
```

**逃生口只有窄化**，没有 C# 的 `!` 那种「我保证非空」后缀——那个后缀正是让问题
从检查里逃掉的口子，所以不提供。认得出的窄化写法：

```z42
if (s != null) { return s.Length; }                    // ✓ 条件成立的那一支
if (s == null) { return 0; }  return s.Length;         // ✓ 早返回守卫（最常用）
if (s == null) { throw new ArgumentException("s"); }   // ✓ throw 同理
return s == null || s.Length == 0;                     // ✓ `||` 短路：右边只在左边为假时求值
return s != null && s.Length > 0;                      // ✓ `&&` 短路
return s != null ? s.Length : 0;                       // ✓ 三元
```

义务点（会触发检查的位置）：成员访问 `s.X`、下标 `a[i]`、方法调用接收者 `s.M()`、
`foreach` 的集合、`throw` 的操作数。**比较不是解引用**——`s == null` 本身永远不会被判红，
整体传递 `Sink(s)` 也不算。

### 标了 `?` 的返回值

调用结果**没有名字可以窄化**，所以逃生口是「先存进局部，再检查那个局部」：

```z42
static string? Find(string k) { … }

C.Find(k).Length;                                  // ✗ E0478
string v = C.Find(k);  if (v != null) { v.Length; } // ✓
```

反过来，**可空的值不许从未标 `?` 的返回类型漏出去**（**E0479**）——未标的返回类型
意味着「调用方不必检查」，放行就等于凭空造一个洞：

```z42
static string  M(string? s) { return s; }      // ✗ E0479
static string? M(string? s) { return s; }      // ✓ 把义务传给调用方
static string  M(string? s) {                  // ✓ 或者在这里先检查
    if (s == null) { return ""; }
    return s;
}
```

> 裸 `return null;`（返回类型未标 `?`）**不报 E0479**。那是「建议给返回类型加 `?`」的
> 反向推导，是另一件事——混进同一个码会让「这里有错」和「这里可以更严」分不开。

**普通局部赋 `null` 不会进入检查范围**：`string v = null;` 里的 `null` 字面量**不是标记**。
义务只由 `?` 产生；赋值只能更新已在检查范围内的名字，不能把新名字拉进来。
否则就等于悄悄打开了「所有引用类型都强制检查」的悲观档——那一档是明确不做的。

> ⚠️ **这不是「空安全」，别当它是。** 它只覆盖标了 `?` 的**形参**、且只认**裸名**
> （`this.field.X` 不在管辖）。已知会漏的形态：同一条语句内 `&&` / `||` / 三元的窄化
> 不回滚，例如 `bool b = s == null || s.Length == 0; return s.Length;` 的第二处不报。
> 取舍是**宁可漏报、绝不误报**——误报会让人不敢信这个诊断。
>
> 标了 `?` 的**字段**（需先定快照规则）、`Expect("理由")` 逃生口，以及反向推导，
> 见变更 `define-null-check-marks` 的后续 PR。

### 「可能没有」的结果怎么表达

按返回类型分流：

| 返回的是 | 写法 |
|---|---|
| **引用类型** | `V? Find(…)` —— 单个可空返回值，**不要** bool |
| **值类型** | `bool TryX(…, ref T v)` —— 失败时出参写零值 |

引用类型自带「缺席」的表示；值类型没有，才需要第二个通道。

```z42
int n;
if (Int32.TryParse(s, ref n)) { use(n); }     // 值类型

IPAddress? a = IPAddress.TryParse(s);          // 引用类型
if (a != null) { use(a); }
```

这样 **bool 与可空值的组合永远不会出现**，也就不会出现「我检查了 bool、编译器还要我检查值」
的双重检查。「总是有，但是多个」用元组：`(int, string) Split(…)`。

## 集合与对象的字面量语法

方括号 `[]` 一律构造**数组**，花括号 `{}` 构造**集合 / 字典 / 对象**。目标类型决定具体容器：

| 形态 | 示例 | 结果 |
|------|------|------|
| 元素列表 | `int[] xs = [1, 2, 3];` | 长度 3 的数组 |
| 重复填充 | `int[] z = [0; 100];` | 100 个 `0`；`0` 只求值一次 |
| 展开拼接 | `int[] c = [..xs, 99];` | `xs` 的元素后接 `99` |
| 空数组 | `int[] e = [];` | 元素类型由目标类型决定 |
| 裸元素 | `List<int> ys = { 1, 2, 3 };` | `List<int>` |
| 键值对 | `Dictionary<string,int> m = { "a": 1 };` | `Dictionary<string,int>` |
| 空花括号 | `List<int> el = {};` | 容器类型由目标类型决定 |
| 显式数组 | `new int[]{ 1, 2 }` / `new int[n]` | 数组 |
| 对象初始化器 | `new Point { X = 1, Y = 2 }` | 构造后逐成员赋值 |
| 字段简写 | `new Point { x, y }` | `x` ≡ `X = x`（同名变量） |
| 带实参 | `new Box(w, h) { Filled = true }` | ctor 实参 + 初始化器 |

```z42
using Std.IO;
using Std.Collections;

void Main() {
    int[] xs = [1, 2, 3];
    int[] z  = [0; 5];
    int[] c  = [..xs, 99];
    List<int> ys = { 1, 2, 3 };
    Dictionary<string,int> m = { "a": 1 };
    Console.WriteLine($"{xs.Length} {z.Length} {c.Length} {ys.Count} {m.Count}");  // 3 5 4 3 1
    Console.WriteLine(c[3]);                                                       // 99
}
```

> ✗ **`new[] { 1, 2, 3 }` 不是 z42 语法**（`new` 后不能直接跟 `[`）。写 `[1, 2, 3]` 或
> `new int[]{ 1, 2, 3 }`。

数组的索引、长度、多维形态，以及集合字面量、对象初始化器的完整规则，分别在
[数组](arrays.md)、[集合字面量](collection-literals.md)、[对象初始化器](object-initializers.md)
各页展开，本页只给语法形态。

## 关联页面

- [类型转换](conversions.md) — 哪些转换是隐式的、哪些要写 `(T)`
- [运算符](operators.md) — `??` / `default(T)` / cast（`?.` 已移除）
- [字符串](strings.md) — 字符串与字符字面量
- [元组](tuples.md) — `(int, string)` 值元组
- [枚举](enums.md) — `enum`
- [内存模型](memory-model.md) — 值语义 / 引用语义 / 装箱
