# 运算符语义

本页讲运算符的**求值规则**：优先级、短路、复合赋值、`default(T)`。类型转换本身的规则
（哪些隐式、哪些要写 `(T)`）在[类型转换](conversions.md)。

## 优先级与结合性

从**紧**到**松**：

| 层级 | 运算符 | 结合性 |
|------|--------|--------|
| 后缀 | `a.b` `a[i]` `f(x)` `a++` `a--` | 左 |
| 前缀 | `-a` `+a` `!a` `~a` `++a` `--a` `(T)a` `new` `typeof` `default` | 右 |
| 乘除 | `*` `/` `%` | 左 |
| 加减 | `+` `-` | 左 |
| 移位 | `<<` `>>` | 左 |
| 关系 / 类型检查 | `<` `<=` `>` `>=` `is` `as` | 左 |
| 相等 | `==` `!=` | 左 |
| 按位与 | `&` | 左 |
| 按位异或 | `^` | 左 |
| 按位或 | `\|` | 左 |
| 条件与 | `&&` | 左 |
| 条件或 | `\|\|` | 左 |
| 条件 | `?:` | 右 |
| 赋值 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` | 右 |

这套次序与 C# 一致，除了 z42 **没有** `??`（已移除，见下）。

```z42
if (a && b || c) { }   // ≡ ((a && b) || c)
int n = 1 | 2 & 3;     // ≡ 1 | (2 & 3) = 3
int m = 1 + 2 << 3;    // ≡ (1 + 2) << 3 = 24
```

## 二元数值提升

两个操作数的数值类型不同时，**在运行期加宽到共同类型后再运算**。加宽规则对
**算术、关系、相等**三类运算符是同一套：

| 操作数 | 加宽到 | 适用 |
|--------|--------|------|
| 整型 × 浮点型 | `f64` | `+` `-` `*` `/` `%`、`<` `<=` `>` `>=`、`==` `!=` |
| `char` × 整型 | `i64`（按码点） | `<` `<=` `>` `>=`、`==` `!=` |

```z42
using Std.IO;

void Main() {
    int i = 5; double d = 5.0; char c = 'A';
    Console.WriteLine(i + d);     // 10   —— 加宽到 f64 后相加
    Console.WriteLine(i == d);    // true
    Console.WriteLine(i <= d);    // true
    Console.WriteLine(c == 65);   // true —— 'A' 的码点
    Console.WriteLine(c < 66);    // true
}
```

注意这与[类型转换](conversions.md)是**两回事**：那里讲的是赋值 / 传参 / 返回等
「协变点」要不要写 `(T)`（`long → double` 在那里是**显式**转换）；二元运算符的操作数不走
协变点，一律运行期加宽，不要求也不插入 `(T)`。

> ⚠️ 加宽到 `f64` 的组合里，`long` / `i64` 超出 53 位尾数时会有舍入：
> `9007199254740993L == 9007199254740992.0` 为 `true`。需要精确比较时别混用类型。

### 装箱后的相等是另一回事

一旦两个值被装箱成 `object`（例如传给 `Assert.Equal(object, object)`），比较的是
**装箱后的类型 + 字节**，不再加宽——`(object)5` 与 `(object)5.0` **不相等**。这与 C# 的
`((object)5).Equals((object)5.0)` 一致。

## 逻辑运算符短路

`&&` 与 `||` **短路求值**——左侧已经决定整体结果时，右侧表达式完全不求值（不触发副作用、
不抛异常）：

```z42
using Std.IO;

class P { public static bool Probe() { Console.WriteLine("probe!"); return true; } }

void Main() {
    int[] arr = null;
    Console.WriteLine(arr != null && arr[0] > 0);   // false —— 不会索引 null
    Console.WriteLine(true || P.Probe());           // true  —— 不打印 probe!
}
```

## 按位运算只接受整数

`&` `|` `^` `~` `<<` `>>` 的操作数**必须是整型**。与 C# 不同，z42 **不允许**把它们用在
`bool` 上作"不短路的逻辑运算"：

```z42
bool x = true & Probe();   // ✗ E0402: operator `&` requires integral operand, got `bool`
```

需要"两侧都求值"的逻辑时，把右侧先算进一个局部变量。

## 复合赋值

八个复合赋值运算符：

| 运算符 | 等价于 | 运算符 | 等价于 |
|--------|--------|--------|--------|
| `+=` | `x = x + rhs` | `%=` | `x = x % rhs` |
| `-=` | `x = x - rhs` | `&=` | `x = x & rhs` |
| `*=` | `x = x * rhs` | `\|=` | `x = x \| rhs` |
| `/=` | `x = x / rhs` | `^=` | `x = x ^ rhs` |

```z42
x += 1;
arr[i] -= 2;
s += " world";     // string 拼接，与 `s = s + " world"` 一致
flags |= 0x4;
```

**不支持**：`<<=`、`>>=`。写了是语法错误（`E0201` unexpected token in expression）。
`??=` 同理不支持——`??` 本身已移除（见下）。

### 两条类型规则

- 操作数的类型要求与对应的二元运算符**完全相同**：`x += y` 按 `x + y` 检查，因此
  `&=` `|=` `^=` 同样只接受整型。
- `string += string` 合法，就是拼接。

> ⚠️ **已知缺口：复合赋值不做窄化检查。** 普通赋值会拦住窄化——
> `x = x + 2.5;`（`x` 是 `int`）报 `E0439`（cannot implicitly convert 'double' to 'Int32'）。
> 但等价的 `x += 2.5;` **能通过编译**，在运行期才失败。
> 混用不同数值类型时，自己写成显式的 `x = x + (int)y;` 形式。

### 支持的左值

局部变量、数组元素、实例字段、嵌套 struct 字段、静态字段、静态属性，包括跨包导入的成员。

### ⚠️ 左值的下标 / 接收者会求值两次

```z42
using Std.IO;

class Counter {
    public static int Side = 0;
    public static int F() { Counter.Side = Counter.Side + 1; return 0; }
}

void Main() {
    int[] arr = [10, 20, 30];
    arr[Counter.F()] += 5;
    Console.WriteLine($"arr0={arr[0]} calls={Counter.Side}");   // arr0=15 calls=2
}
```

结果值是对的，但 `F()` 被调用了 **2 次**。左值里有副作用时，先把它算进一个局部变量：

```z42
int i = Counter.F();
arr[i] += 5;              // F() 只调一次
```

## `event` 上的 `+=` / `-=` 是另一回事

接收者是 `event` 成员时，`+=` / `-=` **不是**读-改-写，而被翻译成订阅 / 退订调用
（`add_X` / `remove_X`），在复合赋值展开**之前**就被拦截：

```z42
c.OnKey += (int x) => Console.WriteLine($"a:{x}");   // 订阅
c.OnKey -= handler;                                   // 退订
```

单播 / 多播的差别、重复绑定抛 `InvalidOperationException` 等规则见
[委托与事件](delegates-events.md)。

## `default(T)`

`default(T)` 求值为类型 `T` 的零值：

| T | `default(T)` |
|---|--------------|
| 所有整型（`int` / `long` / `byte` / `i32` …） | `0` |
| `float` / `double` / `f32` / `f64` | `0.0` |
| `bool` | `false` |
| `char` | `'\0'` |
| `string` | `null` |
| class / interface / 数组 / `T?` | `null` |

```z42
using Std.IO;

void Main() {
    Console.WriteLine(default(int));            // 0
    Console.WriteLine(default(bool));           // false
    Console.WriteLine(default(char) == '\0');   // true
    Console.WriteLine(default(int[]) == null);  // true
}
```

**泛型类型形参**也可以：泛型类的实例方法、泛型方法的 `default(T)` 都会在运行期解析到真实
的类型实参。

```z42
using Std.IO;

class Box<T> { public T Slot() { return default(T); } }
class Util { public static T Make<T>() { return default(T); } }

void Main() {
    Console.WriteLine(new Box<int>().Slot());           // 0
    Console.WriteLine(new Box<string>().Slot() == null); // true
    Console.WriteLine(Util.Make<int>());                // 0
}
```

仍未覆盖的路径：**泛型类上的 static 方法**（`Box<T>` 的 `static T Zero()`）没有接收者也没有
方法级类型实参，`default(T)` 在那里退化为 `null`。

### `default(自定义 struct)`

对值 struct，`default(T)` 产出一个**所有字段都是零值**的 struct——基元叶子为 `0` /
`false` / `'\0'`，引用叶子为 `null`：

```z42
struct Pt { public int X; public int Y; }

var p = default(Pt);        // X=0, Y=0
Console.WriteLine(p.X);     // 0
```

等价于 `new Pt()`。

## 类型检查与转换

- `(T)expr` — 显式转换。
- `expr is T` — 类型检查，返回 `bool`。
- `expr as T` — 尝试转换，失败得 `null`。

**哪些转换是隐式的、哪些必须写 `(T)`、数值溢出与 NaN 的行为、用户自定义 `implicit` /
`explicit operator`** 全部在[类型转换](conversions.md)——z42 的隐式转换集比 C# 更严
（例如 `i32 → f32`、`i64 → f64` 在 z42 是**显式**转换），不要按 C# 的直觉推断。

```z42
using Std.IO;

void Main() {
    long n = (long)3.7;                 // 3，向零截断
    int  c = (int)'A';                  // 65
    char a = (char)65;                  // 'A'
    Console.WriteLine($"{n} {c} {a}");
    Console.WriteLine((int)(0.0 / 0.0));     // 0 —— NaN → 0
    Console.WriteLine((int)100000000000L);   // 1215752192 —— 截低位
}
```

> ⚠️ **`bool` 与数值之间的 cast 编译期不拦截。** `int i = (int)true;` 能通过编译，在运行期抛
> `InvalidCastException`。用条件表达式代替：`int i = flag ? 1 : 0;`。
> `string` 与数值之间同理——用 `Parse` / `ToString`。

## ~~`??`~~ 与 ~~`?.`~~ —— 均已移除（E0480）

空合并 `??` 与空条件成员访问 `?.` **都不再支持**。两者是同一个口子的两种写法：
把「可能为 null」这件事**静默收尾掉**，读代码的人看不见它、崩溃现场也离病因很远。

```z42
var v = n?.value;                                  // ✗ E0480
var t = n;  if (t != null) { var v = t.value; }    // ✓

string s = a ?? b;                                 // ✗ E0480
string s = a;  if (s == null) { s = b; }           // ✓
```

两者都**保留 token**：诊断会带上迁移写法，并把表达式按等价合法形态解析完
（`?.` 按 `.`，`??` 只取左侧），所以不会级联出一串无关的语法错。

### 「读一个设置，取不到用默认值」

这是 `??` 最常见的用途——全仓 **70 处**写的都是
`Environment.GetEnvironmentVariable("X") ?? ""`。这种情况**别在调用点兜底，
让 API 收默认值**：

```z42
string home = Environment.GetEnvironmentVariable("HOME", "");   // 结果保证非空
```

同款：`AppProperties.GetOrDefault(key, fallback)` /
`RuntimeConfig.GetOrDefault(key, fallback)`。

> 顺带修掉的一个真 bug：`?.` 旧的脱糖把接收者**绑定了两次**，所以 `F()?.X` 会
> **调用 `F` 两次**。写成显式检查后，接收者只求值一次。

## 关联页面

- [类型转换](conversions.md) — 隐式 / 显式转换的完整规则
- [基本类型与字面量](types.md) — 各类型的取值范围
- [委托与事件](delegates-events.md) — `event` 上的 `+=` / `-=`
- [模式匹配](pattern-matching.md) — `is` 的模式形态、`switch` 表达式
