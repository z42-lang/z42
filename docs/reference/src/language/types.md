# 基本类型与字面量

z42 的内建标量类型有**两套等价拼写**：C# 风格的关键字（`int`、`double`…）和 Rust 风格的
短名（`i32`、`f64`…）。两者指向同一个类型，可以互相赋值，也**不能**作为重载的区分依据。

## 内建类型总表

| 关键字 | 短名 | 包装类型 | 位宽 | 说明 |
|--------|------|----------|------|------|
| `sbyte` | `i8` | `Std.SByte` | 8 | 有符号 |
| `short` | `i16` | `Std.Int16` | 16 | 有符号 |
| `int` | `i32` | `Std.Int32` | 32 | 有符号 |
| `long` | `i64` | `Std.Int64` | 64 | 有符号 |
| `byte` | `u8` | `Std.Byte` | 8 | 无符号 |
| `ushort` | `u16` | `Std.UInt16` | 16 | 无符号 |
| `uint` | `u32` | `Std.UInt32` | 32 | 无符号 |
| `ulong` | `u64` | `Std.UInt64` | 64 | 无符号 |
| `float` | `f32` | `Std.Single` | 32 | IEEE-754 |
| `double` | `f64` | `Std.Double` | 64 | IEEE-754 |
| `bool` | — | `Std.Boolean` | — | `true` / `false` |
| `char` | — | `Std.Char` | 32 | 一个 Unicode 标量值 |
| `string` | — | `Std.String` | — | 不可变 UTF-8，引用类型 |
| `object` | — | `Std.Object` | — | 所有引用类型的基类，装箱目标 |
| `void` | — | — | — | 仅用于返回类型 |

**短名是规范形式**：编译器把每种拼写归一到短名后再比较类型，因此

```z42
void F(int x)  { }
void F(i32 x)  { }   // ✗ 重复声明——归一后是同一个 F(i32)
```

也因此 `x.GetType().Name` 返回的是包装类型名（`Int32`），`FullName` 是 `Std.Int32`。

> **z42 没有的**：`decimal`、`nint` / `nuint`、`System.*` 的任何类型。

## 字面量

```z42
int    x    = 42;
int    hex  = 0xFF;
long   big  = 9_000_000_000L;   // `_` 数字分隔符；`L` 后缀 = i64
double pi   = 3.14159;          // 无后缀的小数 = f64
float  f    = 1.5f;             // `f` 后缀 = f32
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

## 可空标记 `?`

```z42
string? maybeNull = null;
int?    optInt    = 42;
```

> ⚠️ **当前实现中 `?` 是纯注解，会被类型解析擦除。** z42 **没有** `Nullable<T>` 包装类型，
> 也**没有**可空性流分析：`T?` 和 `T` 解析成同一个类型，编译器不会因为把 `null` 赋给未标 `?`
> 的变量而报错，也不会要求在解引用前检查。
>
> ```z42
> string notNullable = null;   // 编译通过——没有空安全检查
> ```
>
> 因此 `?` 目前的价值是**给读者的意图标注**，不是编译期保证。空引用在运行期解引用时才暴露。

配套的两个运算符按运行期的 null 值工作，是真实生效的：

```z42
string result = maybeNull ?? "default";   // null 合并
int? len      = maybeNull?.Length;        // null 条件访问，左侧为 null 时整体为 null
```

它们的优先级与求值规则见[运算符](operators.md)。

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
- [运算符](operators.md) — `??` / `?.` / `default(T)` / cast
- [字符串](strings.md) — 字符串与字符字面量
- [元组](tuples.md) — `(int, string)` 值元组
- [枚举](enums.md) — `enum`
- [内存模型](memory-model.md) — 值语义 / 引用语义 / 装箱
