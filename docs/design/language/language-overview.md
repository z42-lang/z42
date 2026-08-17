# z42 Language Overview

> **Status**: 参考手册 / Reference Manual ｜ 主题章节链接到具体 design doc；最新覆盖 L1 + lambda（L2-C1）+ delegate（L3-D）

> 本文档是 **L1（Bootstrap）** 阶段的语法实现参考，面向编译器开发者。
> 语言设计决策（feature 层面）见 [`docs/features.md`](../features.md)。
> 演进计划和实现进度见 [`docs/roadmap.md`](../roadmap.md)。
>
> **SoT 关系**：本文档是语法的**叙事性说明**（user-facing prose），机器可读
> 的权威定义在 [`grammar.peg`](grammar.peg)；`Z42.Tests.GrammarSyncTests` 强制
> 校验两者一致。改动语法时按 `grammar.peg` → 本文档 → 跑 `dotnet test
> --filter GrammarSync` 的顺序，避免漂移。

---

## 1. 顶层结构

```z42
namespace Geometry;          // 命名空间声明

using System.Math;           // 导入
using System.Collections.Generic;

// 顶层函数（C# 9+ 风格，无需包在类中）
void Main() {
    var p = new Point(1.0, 2.0);
    Console.WriteLine(p.ToString());
}
```

---

## 2. 基本类型

| z42 类型   | 位宽 | 等价 C# 类型 |
|-----------|------|------------|
| `sbyte`   | 8    | `sbyte`    |
| `short`   | 16   | `short`    |
| `int`     | 32   | `int`      |
| `long`    | 64   | `long`     |
| `byte`    | 8    | `byte`     |
| `ushort`  | 16   | `ushort`   |
| `uint`    | 32   | `uint`     |
| `ulong`   | 64   | `ulong`    |
| `float`   | 32   | `float`    |
| `double`  | 64   | `double`   |
| `bool`    | —    | `bool`     |
| `char`    | 32   | `char` (Unicode) |
| `string`  | —    | `string`   |
| `void`    | —    | `void`     |

```z42
int    x = 42;
long   big = 9_000_000_000L;
double pi = 3.14159;
float  f = 1.5f;
bool   flag = true;
char   ch = 'z';
string s = "hello";

// var 推断
var count = 0;          // int
var name = "z42";       // string
var ratio = 0.5;        // double
```

### 可空类型

```z42
string? maybeNull = null;
int? optInt = 42;

// null 合并
string result = maybeNull ?? "default";

// null 条件访问
int? len = maybeNull?.Length;
```

### 集合字面量（声明与初始化简化，add-collection-literals 2026-08-07）

`[]`＝数组、`{}`＝花括号族（List / Dictionary），借鉴 JSON/JS + C# 集合初始化器 + Rust 重复填充：

```z42
int[]                  xs = [1, 2, 3];        // 数组（方括号一律数组）
int[]                  z  = [0; 100];         // Rust 重复填充（value 只求值一次）
int[]                  c  = [..xs, 99];       // spread 拼接
List<int>              ys = { 1, 2, 3 };      // 花括号 + 裸元素 → List
Dictionary<string,int> m  = { "a": 1 };       // 花括号 + k:v → Dictionary
int[] e = [];  List<int> el = {};             // 空：由目标类型定元素/容器
```

纯前端脱糖（零新 IR / 零格式 bump）。详见 [arrays.md](arrays.md)（`[]`）与
[collection-literals.md](collection-literals.md)（`{}`）。

### 对象初始化器 + 字段简写（add-object-initializers 2026-08-07）

`new Type(args?) { ... }` 构造后逐字段赋值（C#），裸标识符为字段简写（Rust/JS）：

```z42
var p = new Point { X = 1, Y = 2 };        // 对象初始化器
var q = new Point { x, y };                // 字段简写：x ≡ x = x（同名变量）
var b = new Box(w, h) { Filled = true };   // 带 ctor 实参
```

脱糖 `new Foo(args) { X = 1 }` → `$c = new Foo(args); $c.X = 1; $c`（复用集合字面量的 `BoundSeqExpr`，
零新 IR / 零格式 bump）。结构更新 `..base` 延后到 struct 值语义。详见
[object-initializers.md](object-initializers.md)。

---

## 3. 字符串

```z42
string a = "hello";
string b = "world";

// 字符串插值（C# $ 前缀）
string msg = $"Hello, {b}! Length = {b.Length}";

// 原始字符串 `"""..."""`（add-raw-string-literal 2026-05-26）
// - 不解析 `\n` / `\t` 等转义（字面保留 backslash + char）
// - 支持多行 + 嵌入单/双 quote（最多连续 2 个 `"`）
// - 三连 `"""` 即闭合；空字符串写 `""""""`
// - v0 不支持变长 quote 数（C# 11 的 `""""..."""""`）、indent dedent、
//   `$"""..."""` 插值；详见 [`raw-string-literal.md`](raw-string-literal.md)
string json = """
    {
        "key": "value"
    }
    """;

// 转义序列（reject-invalid-string-escape 2026-08-17）
// - 普通串 / char 字面量 / 插值串文本段识别 C 系单字符转义全集：
//   \a(0x07) \b(0x08) \f(0x0C) \n(0x0A) \r(0x0D) \t(0x09) \v(0x0B) \0(0x00) \\ \" \'
// - 未知转义（如 \U \D \q）报编译错误 E0102，不再静默丢反斜杠（对齐 C# CS1009）
// - Windows 路径等含反斜杠的串：用 \\ 转义，或直接用 raw 串 """C:\Users\bin"""（逐字保留）
// - 数字/Unicode 转义 \uXXXX / \xXX 暂不支持（会报 E0102），见 roadmap Deferred
string winpath = "C:\\Users\\bin";      // ✓ 显式 \\
string winpath2 = """C:\Users\bin""";   // ✓ raw 串逐字保留
// string bad = "C:\Users\bin";         // ✗ E0102：\U 是未知转义

// 常用方法
int len = a.Length;
string upper = a.ToUpper();
bool starts = a.StartsWith("he");
string[] parts = a.Split(',');
```

> 完整内置方法清单与 phase 限制见 [`string-builtins.md`](string-builtins.md)。

---

## 3.5 运算符语义

### 逻辑运算符短路求值

`&&` 和 `||` 短路求值 —— 当左侧已决定整体结果时，**不**对右侧表达式求值（不触发副作用、不抛出异常）：

```z42
// `&&` 左侧为 false → 右侧跳过
bool safe = arr != null && arr[0] > 0;   // arr 为 null 时不会索引

// `||` 左侧为 true → 右侧跳过
bool ok = cached || Probe();             // 已命中缓存时 Probe() 不执行

// 优先级：&& 比 || 紧
if (a && b || c) { ... }                 // 等价 ((a && b) || c)
```

位运算 `&` / `|` **不**短路，两侧总是求值。

实现注：IR 层将 `&&` / `||` desugar 为 `BrCond` 控制流；保留 `AndInstr` / `OrInstr` 仅用于位运算。

### `default(T)` zero-value 表达式

`default(T)` 求值为类型 T 的零值，与 C# 语义一致；常用于 reset 字段、初始化容器槽位、以及"无意见的占位值"。

```z42
var i  = default(int);       // 0
var d  = default(double);    // 0.0
var b  = default(bool);      // false
var c  = default(char);      // '\0'
var s  = default(string);    // null（reference type 默认；用 `?? ""` 选 empty 语义）
var p  = default(Point);     // null（任意 class / interface / array / nullable）
var a  = default(int[]);     // null（与 `new int[N]` 区分；后者分配 N 个零）
```

| T | `default(T)` |
|---|------|
| `int` / `long` / `byte` / `short` 等所有整型别名 | `0`（VM `Value::I64(0)`）|
| `double` / `float` / `f32` / `f64` | `0.0` |
| `bool` | `false` |
| `char` | `'\0'`（NUL 字符）|
| `string` | `null` |
| 任意 class / interface / array / `T?` / 自定义 struct | `null` |

实现注：fully-resolved T → IR 层不引入新指令；编译器按 T 直接 emit `ConstI64` / `ConstF64` / `ConstBool` / `ConstChar` / `ConstNull`，与 VM `default_value_for(type_tag)` 表对齐。

**泛型 type-parameter 支持**（Phase 2，2026-05-07 add-default-generic-typeparam）：在 generic class `Foo<T>` 的 instance method / ctor body 内 `default(T)` 走运行时解析路径 —— IR emit 新指令 `DefaultOf(dst, param_index)`（`param_index` 是 T 在类 type_params 中的 0-based 位置），VM 读 `frame.regs[0]`（this）的 `ScriptObject.type_args[idx]` → 走 `default_value_for(tag)`。
`new Foo<int>()` 的实例 type_args = `["int"]`；`Foo<int>::Make()` 内 `default(T)` 真返 `0`，`Foo<string>` 实例返 `null`。

边界：method-level type-param `m<U>()`、free generic function `f<T>()`、static method on generic class — 这些路径无 `this`，编译期通过但运行时退化为 `Value::Null`（graceful-degradation）。后续 spec 拓展 calling convention 让方法级 / free 也能传 type_args。

### 显式数值 cast（spec fix-numeric-cast-lowering, 2026-05-13）

C# 风格 `(T)expr` 表达式做显式类型转换。**合法**、**非法** 两个矩阵：

**合法 cast**：

| from → to | 行为 |
|---|---|
| `double → long/int/short/byte/...` | 向零截断；NaN → 0；超界 saturating（Rust `as iN` 语义）|
| `float/double → long/int` | 同上 |
| `long → int/short/byte` | 截低位 + 符号扩展，与 C# `unchecked` 一致：`(int)100000000000L == 1215752192` |
| `int/long → double/float` | widening；超 2^53 时 f64 精度可能损失（沉默） |
| `char → int/long` | 取 Unicode scalar value：`(int)'A' == 65` |
| `int/long → char` | 校验有效 Unicode scalar（拒 surrogate / >U+10FFFF；运行期 `InvalidCastException`） |
| `object → 任意数值/char` | 运行时按 `Value` variant 解析；现有 `(long)raw[0]` 习惯继续可用 |

**非法 cast → 编译期 E0424**：
- `bool ↔ 数值/char/string` — 用条件表达式替代
- `string ↔ 数值/char` — 用 `Parse` / `ToString` 替代

```z42
long n = (long)3.7;         // 3
int  c = (int)'A';          // 65
char a = (char)65;          // 'A'

int  i = (int)true;         // E0424：cannot cast bool ↔ numeric
long n = (long)"42";        // E0424：use long.Parse
```

**身份 cast**（如 `(int)int_var`）零开销 — IR 层不发射任何指令。

**实现备注**：IR 新增 `Convert(dst, src)` 指令；dst 静态类型标记目标，src 运行时 `Value` variant 决定来源。运行时 saturating 与 NaN→0 沿用 Rust `as` 语义（与 z42 现有 `int_binop` `wrapping_*` 风格一致），未来若需 C# `checked` 严格语义可独立 spec 升级。

---

## 4. 控制流

```z42
// if / else
if (x > 0) {
    Console.WriteLine("positive");
} else if (x < 0) {
    Console.WriteLine("negative");
} else {
    Console.WriteLine("zero");
}

// while / do-while
while (count < 10) {
    count++;
}

do {
    count--;
} while (count > 0);

// for
for (int i = 0; i < 10; i++) {
    Console.WriteLine(i);
}

// foreach —— 支持数组、字符串、以及任意实现 `int Count()` + `T get_Item(int)` 的类
var numbers = new[] { 1, 2, 3, 4, 5 };
foreach (var n in numbers) {
    Console.WriteLine(n);
}

// 用户类容器（duck-typed 协议，无需显式实现 IEnumerable）
var xs = new ArrayList<int>();
xs.Add(1); xs.Add(2);
foreach (var v in xs) { Console.WriteLine(v); }

// switch 表达式（C# 8+）
string label = x switch {
    > 0  => "positive",
    < 0  => "negative",
    _    => "zero"
};

// switch 语句
switch (direction) {
    case Direction.North:
        Console.WriteLine("going north");
        break;
    default:
        Console.WriteLine("other direction");
        break;
}
```

---

## 5. 函数与方法

```z42
// 顶层函数
int Add(int a, int b) {
    return a + b;
}

// 表达式体（C# expression-bodied）
int Multiply(int a, int b) => a * b;

// 默认参数
void Greet(string name, string prefix = "Hello") {
    Console.WriteLine($"{prefix}, {name}!");
}

// 具名实参（C# 风格）：按参数名传递，可乱序；位置实参必须在具名实参之前
// 跳过中间默认参数也使用具名形式
void Draw(string color, int width = 1, bool filled = false) { }
Draw(color: "red", filled: true);          // width 走默认；filled 跳过 width 用具名
Draw("blue", filled: true);                // 混合：color 位置 + filled 具名
// Draw(color: "red", 2);                  // 错误 Z1001：位置实参不能跟在具名实参后
// Draw(badName: "red");                   // 错误 Z1002：参数名不存在
// Draw(color: "red", color: "blue");      // 错误 Z1003：具名实参重复
// Draw("red", color: "blue");             // 错误 Z1004：第 1 个参数被位置+具名重复指定
// Draw(width: 2);                         // 错误 Z1005：required 参数 color 未提供

// 可变参数（详见 5.0.5）
int Sum(params int[] values) {
    int total = 0;
    foreach (var v in values) total += v;
    return total;
}
Sum(1, 2, 3);           // expanded form：散列实参自动打包为 int[]
Sum(new int[] { 1, 2 }); // normal form：直接传数组

// 多返回值：tuple（推荐）
(bool ok, int v) TryParse(string s) {
    return (true, 42);
}
var (ok, v) = TryParse("42");

// 参数修饰符：ref / out / in（编译期已落地，运行时实施在 follow-up spec
// `impl-ref-out-in-runtime`；详见 docs/design/language/parameter-modifiers.md）
void Increment(ref int x) { x = x + 1; }    // 双向引用
bool TryParseOut(string s, out int v) {     // 单向输出 + DefiniteAssignment
    v = 0; return false;
}
double Norm(in BigVec v) { ... }            // 只读引用，零拷贝

// callsite 三者均强制写修饰符（修正 C# `in` 可省的不一致）：
var c = 0;
Increment(ref c);
TryParseOut("x", out var n);                // out var 内联声明
var d = Norm(in someVec);

```

> **过渡期注意**：编译期对 `ref` / `out` / `in` 的所有验证已生效（修饰符一致 / lvalue / DA / 4 交互规则 / overload）。运行时 callee 修改尚未传回 caller —— 见 `parameter-modifiers.md` "Runtime Implementation"。需要"修改调用方变量"语义时优先用 tuple 多返回值。

### 5.0 方法重载决议（type-based，对齐 C# 子集）

同名方法允许 **arity 相同、形参类型不同**（此前仅按 arity 区分，同 arity 的重载会静默丢失其中一个）：

```z42
static void Handle(int n) { ... }
static void Handle(string s) { ... }   // 与上面同 arity（1），按类型区分，合法重载

class Vec {
    public static Vec operator +(Vec a, Vec b) { ... }
    public static Vec operator +(Vec a, int scalar) { ... }   // 同 arity 操作符重载同样合法
}
```

**决议算法**（调用点按实参类型在候选集里择优）：
1. **适用性**：实参数量匹配 arity，且每个实参类型可赋值到对应形参类型（含子类→基类、原始类型→`object` 装箱、接口实现）
2. **最具体优先**：精确类型匹配优于加宽/装箱匹配；候选 A 若在每个参数位置都不差于候选 B、且至少一处更精确，则 A 胜出
3. 适用集为空 → 编译错误（无匹配重载）；无法分出唯一最优（多个加宽候选并列）→ 编译错误（歧义重载，要求显式 cast 消歧）

**v1 收窄**：不做 `int→long→double` 等隐式数值加宽的完整优先级排序——多个数值加宽候选并列即报歧义。

**不视为合法重载**：仅 nullable 或类型别名不同的"重载"（如 `F(string)` 与 `F(string?)`，或 `F(int)` 与
`F(i32)`）会被当作重复声明报错，而不是两个不同的重载——这两种情况类型归一后是同一个类型。

**实例方法**同样支持 type-based 重载，但以下 6 个方法名固定按 arity 区分（不参与类型区分，同 arity 的
第二个声明仍会静默覆盖第一个，与协议方法的历史行为一致）：`ToString`、`Equals`、`GetHashCode`、
`GetType`、`get_Item`、`set_Item`。

**virtual / override**：override 方法的形参类型必须与被覆盖的虚方法完全一致（C# 子集语义）；无论
基类是否声明了其它同 arity 的重载，子类的 override 都会正确复用基类对应重载的虚函数表槽位。

### 5.0.5 变长参数（`params`）

`params` 关键字放在方法/函数最后一个参数前，标记该参数接受**变长实参**；参数类型必须是 `T[]`（数组）：

```z42
// typed overload —— 无 boxing，同构参数最优性能
public static string Join(string sep, params string[] parts) { ... }

// object overload —— 混类型参数（借助 boxing）
public static void Describe(params object[] args) { ... }
```

**调用形态**（二选一，由实参形状决定，编译期静态决议）：
- **Expanded form**：`Join(",", "a", "b", "c")` —— 散列实参在调用点由编译器隐式打包为 `new string[] { "a", "b", "c" }`
- **Normal form**：`Join(",", new string[] { "a", "b" })` —— 直接传一个数组，跳过打包

**重载决议**（对齐 C# 语义）：
1. Normal form（单个 `T[]` 实参且类型精确匹配）优先于 expanded form
2. 两个 `params` 重载均以 expanded form 适用时，element type 更具体的胜出：`params string[]` 优于 `params object[]`（string 精确匹配，无需装箱）
3. 调用方实参类型混杂（如 `f(1, "x")`）：不满足 `params string[]` 的 expanded 适用性，fallback 到 `params object[]`（逐参数装箱）
4. 允许 `params` 参数前有普通定参（如 `Join(string sep, params string[] parts)`），定参部分按普通规则参与决议

**IR / 跨包**：纯编译器前端 lowering，**零新 IR 指令、VM 不感知 `params`**——expanded form 在 Codegen 阶段展开为 `new T[n]` + 逐元素存入 + 既有 `Call`/`VCall`/`CallIndirect` 指令。跨 zpkg 调用时，`params` 信息（携带哪个形参 + 元素类型）作为 TSIG 的一部分序列化，供导入方在本地重新做 normal/expanded 决议。

### 5.1 局部函数（嵌套函数声明，C# 7+）

```z42
int Outer() {
    // 局部函数：仅在 Outer 内可见，可直接递归
    int Helper(int x) => x * 2;
    int Fact(int n) => n <= 1 ? 1 : n * Fact(n - 1);
    return Helper(3) + Fact(5);
}
```

L2 阶段嵌套函数不允许引用外层 local（捕获是 L3 的闭包特性）；L3 阶段引用外层 local 时升级为闭包，详见 [`closure.md`](closure.md)。

### 5.2 Lambda 与函数类型

z42 提供 C# 风格 lambda 字面量与 **`(T) -> R` 函数类型**（替代 `Func<T,R>` / `Action<T>`）。

```z42
// Lambda 字面量
var inc = (int x) => x + 1;                  // 表达式 body
var step = (int x) => {                      // 语句 body
    var y = x * 2;
    return y + 1;
};

// 函数类型作为参数 / 字段类型
List<U> Map<T, U>(List<T> self, (T) -> U f) { ... }
class EventBus {
    public List<(int) -> void> Handlers = new();
}

// 高阶 API 用法
list.Map(x => x * 2);
list.Filter(x => x > 0);
```

**捕获语义**（L3 完整闭包阶段）：值类型按快照、引用类型按身份、`spawn` 边界 move + Send。完整规范见 [`closure.md`](closure.md)。

L2 阶段编译器只接受**无捕获 lambda**——回调字面量（如 `x => x.Name`）可用，但引用外层 local 直接编译错误。

---

## 6. 类

### 6.1 Object 基类与 Type 描述符

所有 z42 **引用类型**（`class`）均隐式继承 `Std.Object`（对应 `z42.core/Object.z42`）。
编译器在 TypeCheck 和 IrGen 阶段自动注入 `base_class: "Std.Object"`，VM 在 `build_type_registry`
时将 Object 的虚方法（`ToString`/`Equals`/`GetHashCode`）加入 vtable，派生类可通过 `override` 重写。
**值类型**（`struct`、`record`）不继承 Object，编译器为其自动合成值语义的 `Equals`/`GetHashCode`/`ToString`。

`Object` 提供以下成员：

| 成员 | 签名 | 行为 |
|------|------|------|
| `GetType()` | `extern Type GetType()` | 返回运行时 `Type` 描述符（VM 提供 `__obj_get_type`） |
| `ReferenceEquals` | `static extern bool ReferenceEquals(Object? a, Object? b)` | 堆地址相等（两个 null 也为 true） |
| `Equals` | `virtual extern bool Equals(Object? other)` | 默认引用相等（`__obj_equals`）；子类可重写为值相等 |
| `GetHashCode` | `virtual extern int GetHashCode()` | 基于 Rc 指针地址的 identity hash（`__obj_hash_code`）；重写 `Equals` 时必须同步重写 |
| `ToString` | `virtual extern string ToString()` | 默认返回不含命名空间的类名（`__obj_to_str`）；子类通常应重写 |

`Type` 是轻量的运行时类型描述符，仅可通过 `GetType()` 获取，不可直接构造：

```z42
var t = obj.GetType();
Console.WriteLine(t.Name);      // "Circle"
Console.WriteLine(t.FullName);  // "geometry.Circle"
```

**规则：**
- 重写 `Equals` 时必须同时重写 `GetHashCode`，两者必须保持一致。
- `ReferenceEquals` 不可被重写（静态方法）。
- `ToString()` 默认返回不含命名空间的类名；需要完全限定名时用 `GetType().FullName`。

### 6.2 类定义示例

```z42
public class Point {
    // 属性（C# auto-property）
    public double X { get; set; }
    public double Y { get; set; }

    // 构造函数
    public Point(double x, double y) {
        X = x;
        Y = y;
    }

    // 主构造函数（C# 12+）
    // public class Point(double X, double Y) { ... }

    // 方法
    public double DistanceTo(Point other) {
        double dx = X - other.X;
        double dy = Y - other.Y;
        return Math.Sqrt(dx * dx + dy * dy);
    }

    // 重写 ToString
    public override string ToString() => $"Point({X}, {Y})";

    // 静态工厂方法
    public static Point Origin() => new Point(0, 0);
}

// 继承
public class Point3D(double X, double Y, double Z) : Point(X, Y) {
    public double Z { get; set; } = Z;

    public override string ToString() => $"Point3D({X}, {Y}, {Z})";
}
```

### 6.3 字段默认值与初始化器

实例字段（不是 auto-property）支持声明时附加 `=` 初始化器：

```z42
class Box {
    int n = 5;            // 显式 initializer
    string s = "hello";
    bool flag;            // 无 initializer → 取类型默认值
}
```

规则（参见 `docs/spec/archive/2026-05-02-fix-class-field-default-init/`）：

| 字段写法 | 初始值（无显式 ctor）| 初始值（显式 ctor body 覆写后）|
|---------|--------------------|-----------------------------|
| `int n;` | `0` | ctor body 内最后一次赋值 |
| `int n = 5;` | `5`（合成 ctor 入口注入 init）| `5` → 用户 body 的赋值覆写 |
| `bool flag;` | `false` | … |
| `string s;` | `null` | … |
| `string s = "x";` | `"x"` | … |
| `Point p;` | `null`（引用类型默认）| … |

**实现细节**：

- 编译器把 instance field initializer 注入到每个显式 ctor 入口（base ctor call 之后、用户 body 之前）。
- 类没有显式 ctor 但本类或本地祖先链上任一类有字段 init → 编译器合成无参隐式 ctor，按祖先 → 自身顺序内联整条链的 field init 表达式。
- 字段无 init 时，VM `ObjNew` 按字段声明类型选默认值（`int*`/`f64*` → 0、`bool` → false、`char` → `'\0'`、`str`/引用类型 → null），不再一律 null。
- z42 当前模型不自动调用 base ctor — 显式 ctor 仍需用户主动写 `: base(...)` 触发父类 ctor side effect；合成 ctor 仅内联本地祖先字段 init，不调用任何 base ctor。

---

## 7. 结构体

值类型，分配在栈上，赋值时复制。

```z42
public struct Color {
    public byte R { get; }
    public byte G { get; }
    public byte B { get; }

    public Color(byte r, byte g, byte b) => (R, G, B) = (r, g, b);

    public static readonly Color White = new Color(255, 255, 255);
    public static readonly Color Black = new Color(0, 0, 0);

    public override string ToString() => $"#{R:X2}{G:X2}{B:X2}";
}

// 使用
var red = new Color(255, 0, 0);
var copy = red;     // 值拷贝
```

---

## 8. Record

不可变数据类型，自动生成相等性比较、`ToString`、解构。

```z42
// record class（引用语义）
public record Person(string Name, int Age);

// record struct（值语义）
public record struct Vector2(double X, double Y);

// 使用
var alice = new Person("Alice", 30);
var older = alice with { Age = 31 };    // 非破坏性更新

Console.WriteLine(alice);              // Person { Name = Alice, Age = 30 }
Console.WriteLine(alice == older);     // false
```

---

## 9. 接口

```z42
public interface IShape {
    double Area();
    double Perimeter();
    string Name { get; }
}

public interface IDrawable {
    void Draw();
}

// 多接口实现
public class Circle : IShape, IDrawable {
    public double Radius { get; }

    public Circle(double radius) {
        Radius = radius;
    }

    public double Area()      => Math.PI * Radius * Radius;
    public double Perimeter() => 2 * Math.PI * Radius;
    public string Name        => "Circle";

    public void Draw() {
        Console.WriteLine($"Drawing {Name} r={Radius}");
    }
}
```

---

## 10. 枚举

```z42
// 简单枚举
public enum Direction {
    North, South, East, West
}

// 带底层值的枚举
public enum StatusCode : int {
    Ok      = 200,
    NotFound = 404,
    Error   = 500
}

Direction dir = Direction.North;
StatusCode code = StatusCode.Ok;

// 枚举作为 switch 目标
string label = dir switch {
    Direction.North => "↑",
    Direction.South => "↓",
    Direction.East  => "→",
    Direction.West  => "←",
    _               => "?"
};
```

---

## 11. 判别联合（代数类型）

Phase 1 使用 C# abstract record 层次结构模拟。

```z42
public abstract record Shape;
public sealed record Circle(double Radius) : Shape;
public sealed record Rectangle(double Width, double Height) : Shape;
public sealed record Triangle(double Base, double Height) : Shape;

// 模式匹配
double Area(Shape s) => s switch {
    Circle c        => Math.PI * c.Radius * c.Radius,
    Rectangle r     => r.Width * r.Height,
    Triangle t      => 0.5 * t.Base * t.Height,
    _               => throw new ArgumentException($"Unknown shape: {s}")
};

// 解构模式
if (s is Circle { Radius: > 10 } big) {
    Console.WriteLine($"Big circle: r={big.Radius}");
}
```

---

## 12. 异常处理

```z42
// throw
void Validate(int age) {
    if (age < 0) throw new ArgumentException($"Invalid age: {age}");
}

// try / catch / finally
try {
    var result = Divide(10, 0);
} catch (DivideByZeroException ex) {
    Console.WriteLine($"Error: {ex.Message}");
} catch (Exception ex) when (ex.Message.Contains("overflow")) {
    Console.WriteLine("Overflow detected");
} finally {
    Console.WriteLine("cleanup");
}

// 自定义异常
public class Z42RuntimeException(string message, int errorCode)
    : Exception(message) {
    public int ErrorCode { get; } = errorCode;
}
```

> 完整异常类层次、catch 类型过滤、Phase 1 限制（StackTrace / 构造器重载等）见 [`exceptions.md`](exceptions.md)。

---

## 13. 执行模式注解（z42 扩展）

z42 在 C# 基础上新增执行模式注解，对 VM 行为进行提示：

```z42
[ExecMode(Mode.Interp)]        // 始终解释执行（快速启动、热重载）
namespace Scripts.Config;

[ExecMode(Mode.Jit)]           // JIT 编译（最优吞吐）
namespace Engine.Render;

[ExecMode(Mode.Aot)]           // AOT 编译（确定性性能）
namespace Core.Crypto;
```

跨模式调用透明，像普通方法调用一样。

---

## 14. 热更新注解（z42 扩展）

`[HotReload]` 注解标记的命名空间支持运行时函数替换，无需重启 VM。面向游戏脚本等需要快速迭代的场景。

```z42
[HotReload]
namespace Game.Scripts;

void OnUpdate(float dt) { ... }   // 热更新后下一次调用即生效
```

与 `[ExecMode(Mode.Interp)]` 配套使用；JIT/AOT 模式不支持热更新。

详见 `specs/hot-reload.md`。

## 15. InternalCall 互操作（`extern` + `[Native]`）

z42 通过 `extern` 关键字 + `[Native("__intrinsic")]` 属性声明 VM 内建函数的绑定，实现标准库与 VM 的零开销互操作（InternalCall 机制）。

```z42
namespace Std.IO;

public static class Console {
    // 声明：VM 实现，无 z42 函数体
    [Native("__println")]
    public static extern void WriteLine(string value);

    [Native("__readline")]
    public static extern string ReadLine();
}
```

**规则：**

- `extern` 方法必须同时带 `[Native("__name")]` 属性；缺少属性报 `Z0903`
- `[Native("__name")]` 属性必须在 `extern` 方法上使用；缺少 `extern` 报 `Z0904`
- `__name` 必须是 VM 已注册的内建名（见 `NativeTable.All`）；未知名报 `Z0901`
- 参数数量必须与 `NativeTable` 中的定义一致；不符报 `Z0902`
- `extern` 方法不允许有函数体（使用 `;` 代替 `{}`）

**IR 映射：** 编译器将 `extern` 方法编译为单块函数，函数体为一条 `Builtin` 指令 + `Ret`：

```
function z42.io.Console.WriteLine(param_count=1) -> void
  entry:
    r1 = builtin "__println" [r0]
    ret
```

**方法体语法糖（表达式体）：** 非 `extern` 方法支持 `=> expr;` 形式作为函数体简写：

```z42
public static void Log(string msg) => Console.WriteLine(msg);
// 等价于 { Console.WriteLine(msg); }
```

---

## 16. 编译器错误恢复

z42 编译器支持**多错误报告**（error recovery）：解析器遇到语法错误后不立即停止，而是恢复到下一个恢复点并继续解析，从而在单次编译中报告多个错误。

**恢复点层级（从粗到细）：**

| 层级 | 恢复位置 | 说明 |
|------|----------|------|
| 顶层声明 | 下一个 `class`/`struct`/`enum`/`void`/类型关键字 | 一个声明解析失败后继续解析下一个 |
| 类体成员 | 下一个 `;` 或 `}` | 一个字段/方法失败后继续解析下一个成员 |
| 枚举成员 | 下一个 `,` 或 `}` | 枚举成员修饰符等错误后跳过该成员 |
| 语句 | 下一个 `;` / `}` / 语句关键字 | 一条语句失败后继续解析同一块的后续语句 |

**AST 占位节点：**
- `ErrorExpr` — 表达式解析失败时插入，TypeChecker 将其类型推断为 `Error`，Codegen 生成空常量
- `ErrorStmt` — 语句解析失败时插入，Codegen 跳过

**调用方式：**
```csharp
// 推荐：不捕获异常，通过 Diagnostics 检查
var cu = parser.ParseCompilationUnit();
if (parser.Diagnostics.HasErrors) { /* 处理错误 */ }
```

> 错误恢复是尽力而为的机制，用于改善开发体验。级联错误（cascade errors）可能出现，但编译器保证不会因错误恢复陷入死循环。

---

> IR 映射细节（`do-while`、`??`、`?.`、`enum` 编译策略、`List<T>`/`Dictionary<K,V>` 内置方法）见 [`docs/design/runtime/ir.md`](../runtime/ir.md)。
