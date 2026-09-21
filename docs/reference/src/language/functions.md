# 函数与方法

函数的声明形式、参数与返回值、重载决议、局部函数与 lambda。

## 声明形式

函数可以写在**文件顶层**（自由函数），也可以写在类型里（方法）。两者的签名语法一致：
返回类型在前、参数表在后。

```z42
using Std.IO;

// 顶层函数
int Add(int a, int b) {
    return a + b;
}

// 表达式体（`=> expr;` 代替 `{ return expr; }`）
int Multiply(int a, int b) => a * b;

void Main() {
    Console.WriteLine(Add(1, 2).ToString());       // 3
    Console.WriteLine(Multiply(3, 4).ToString());  // 12
}
```

表达式体同样适用于 `void` 方法（此时 `=>` 后是一条表达式语句）：

```z42
public static class Diag {
    public static void Log(string msg) => Console.WriteLine(msg);
}
```

`extern` 方法（`[Native(...)]` 绑定到宿主实现）没有函数体，`)` 之后直接写 `;`。

> ### 自由函数参与重载
>
> 同一命名空间下同名顶层函数按**参数类型序列**区分身份，与类型成员方法走同一套重载决议：
>
> ```z42
> int F(int a)    { return a; }
> int F(string s) { return 1; }   // ✓ 合法重载：按实参类型选一个
> ```
>
> 只有**签名完全相同**（同名、同参数类型；形参名与返回类型不算区别）才是重复声明，报
> `E0408`。重载决议规则见下文「重载决议」。

## 参数

### 默认值

```z42
void Greet(string name, string prefix = "Hello") {
    Console.WriteLine($"{prefix}, {name}!");
}

Greet("Alice");                 // Hello, Alice!
Greet("Bob", prefix: "Hi");     // Hi, Bob!
```

把带默认值的参数排在最后是惯例——z42 **目前不强制**这个顺序
（`F(int a = 1, int b)` 能编译），但可选参夹在中间时，位置调用没法跳过它，
只能靠[命名实参](named-arguments.md)。

### 命名实参

任何形参都可以按名字传，命名实参之间可乱序、可跳过中间的可选参。完整规则见
[命名实参](named-arguments.md)。

### `params` 变长参数

`params` 放在**最后一个**参数之前，该参数的类型必须是数组 `T[]`：

```z42
class Text {
    public static string Join(string sep, params string[] parts) { return "typed"; }
    public static string Join(string sep, params object[] parts) { return "object"; }
}
```

调用点有两种形态，由实参形状在编译期静态决定：

| 形态 | 写法 | 含义 |
|---|---|---|
| **expanded** | `Join(",", "a", "b", "c")` | 散列实参由编译器打包成 `new string[] { "a", "b", "c" }` |
| **normal** | `Join(",", new string[] { "a", "b" })` | 直接传一个数组，不打包 |

`params` **必须是最后一个参数**（否则 `E0206`），且**不能**同时带 `ref` / `out` / `in`
或默认值（`E0208: 'params' cannot combine with 'ref'/'out' or a default value`）。

`params` 与重载一起用时的优先级：

1. **normal form 优先**：单个实参且类型与 `T[]` 精确匹配时，直接当数组传，不展开。
2. 两个 `params` 重载都以 expanded form 适用时，**元素类型更具体的胜出**——
   `params string[]` 优于 `params object[]`（前者精确匹配，无需装箱）。
3. 实参类型混杂（`Join(",", 1, "x")`）不满足 `params string[]` 的适用性，回落到 `params object[]`，逐实参装箱。
4. `params` 之前可以有普通定参，定参部分按普通规则参与决议。

### `ref` / `out` / `in`

三个修饰符写在参数类型之前，调用点必须重复写出修饰符：

```z42
void Increment(ref int x) { x = x + 1; }
bool TryTake(string s, out int v) { v = s.Length; return true; }

int c = 0;
Increment(ref c);              // c == 1，callee 的写入会传回调用方
TryTake("hey", out var n);     // out var 内联声明
```

适用范围、写回的边界与已知缺口见[参数修饰符 `ref` / `out` / `in`](parameter-modifiers.md)。

## 返回值

单返回值照常写。需要**多返回值**时用元组：

```z42
(bool, int) TryParseTwo(string s) {
    return (true, 42);
}

(ok, v) = TryParseTwo("42");     // 解构：声明两个新局部 ok / v
Console.WriteLine($"{ok} {v}");  // true 42
```

> 元组**元素没有名字**：`(bool ok, int v) TryParse(...)` 这种具名元素语法不被接受。
> 解构写的是裸 `(ok, v) = expr;`——前面**不加** `var`。
> 元组的类型、赋值与限制见[元组](tuples.md)。

## 重载决议

同一个类里的同名方法允许**同 arity、参数类型不同**：

```z42
class Handler {
    public static void Handle(int n)    { Console.WriteLine($"int {n}"); }
    public static void Handle(string s) { Console.WriteLine($"string {s}"); }
}

class Vec {
    public int X;
    public Vec(int x) { X = x; }
    // 运算符重载同样可以同 arity
    public static Vec operator +(Vec a, Vec b)      { return new Vec(a.X + b.X); }
    public static Vec operator +(Vec a, int scalar) { return new Vec(a.X + scalar); }
}
```

调用点按实参类型在候选集里择优：

1. **适用性**：实参个数与 arity 匹配，且每个实参类型可落到对应形参类型上。
2. **最具体优先**：精确匹配优于加宽 / 装箱匹配；候选 A 若在每个参数位上都不差于 B、
   且至少一处更精确，则 A 胜出。
3. 适用集为空 → 编译错误（找不到该方法）；无法分出唯一最优 → 歧义重载 `E0425`，
   需要显式转换或改用命名实参消歧。

命名实参与省略的默认实参都参与选重载，规则见[命名实参](named-arguments.md)。

### 不算合法重载的两种写法

只在**可空性**或**类型别名**上不同的两个签名，归一后是同一个类型，按重复声明报 `E0408`：

```z42
class C {
    public int F(int a) { return a; }
    public int F(i32 a) { return a; }   // ✗ E0408: parameter types are identical once
                                        //   aliases (e.g. int/i32) and nullability are normalized
}
```

`F(string)` 与 `F(string?)` 同理。

### `virtual` / `override`

`override` 方法的形参类型必须与被覆盖的虚方法**完全一致**。基类即使声明了其它同 arity 的重载，
子类的 `override` 也会正确接管对应那一个重载的虚表槽位。

### 上溯匹配：子类 → 基类 / 接口

实参可以是形参类型的**派生类**或**实现类**，重载决议会沿继承链与接口表判定：

```z42
interface IMark { void Mark(); }
class A { }
class M : A { }
class D : M, IMark { public void Mark() { } }

class C {
    public static int F(A x)      { return 1; }
    public static int F(string s) { return 2; }

    public static int G(A x)      { return 1; }
    public static int G(M x)      { return 2; }

    public static int K(A x)      { return 1; }
    public static int K(object o) { return 2; }
}

C.F(new D());   // 1 —— 沿 D → M → A 上溯
C.G(new D());   // 2 —— 两个都适用时，**更派生**的形参胜
C.K(new D());   // 1 —— 具体基类优于 object
```

跨包同样成立（依赖包里定义的类层次，消费方一样能沿链判定）。

**两者都适用且无法比较**时是歧义，报 `E0425`：

```z42
class C2 {
    public static int F(A p)     { return 1; }
    public static int F(IMark p) { return 2; }
}

C2.F(new D());  // ✗ E0425: ambiguous call ... add an explicit cast to disambiguate
                //   （D 既是 A 的派生类、又实现了 IMark，两条路不可比）
```

显式转换即可消歧：`C2.F((A)new D())`。

### 调用点的实参个数诊断

| 情形 | 诊断 |
|---|---|
| 缺少无默认值的形参 | `E1005` |
| 实参个数多于形参（非 `params` 调用） | `E1006` |

```z42
class C { public static int F(int a, int b) { return a; } }
C.F(1);          // ✗ E1005: `C.F` is missing required argument `b` (no default value)
C.F(1, 2, 3);    // ✗ E1006: too many arguments to `C.F`: expects 2
```

## 局部函数

方法体内可以声明只在该方法内可见的函数，支持直接递归，也**可以捕获外层局部变量**：

```z42
int Outer() {
    int scale = 3;
    int Helper(int x) => x * scale;                       // 捕获外层 scale
    int Fact(int n) => n <= 1 ? 1 : n * Fact(n - 1);      // 直接递归
    return Helper(4) + Fact(5);                           // 12 + 120 = 132
}
```

## Lambda 与函数类型

z42 用 **`(T) -> R`** 表示函数类型（没有 `Func<>` / `Action<>` 这类泛型委托作为基础形式），
配 C# 风格的 lambda 字面量。注意函数类型用细箭头 `->`，lambda 用粗箭头 `=>`。

```z42
// 函数类型的局部变量
(int) -> int sq = (int x) => x * x;
Console.WriteLine(sq(7).ToString());        // 49

// 语句体 lambda
var step = (int x) => {
    var y = x * 2;
    return y + 1;
};

// 函数类型作为形参
int Apply(int v, (int) -> int f) { return f(v); }
Apply(5, x => x + 1);                       // 6：形参类型已知时可省略 lambda 参数类型
```

无返回值的函数类型写成 `(int) -> void`，可以作为字段 / 集合元素类型：

```z42
class EventBus {
    public List<(int) -> void> Handlers = new();
}

var bus = new EventBus();
bus.Handlers.Add((int x) => Console.WriteLine($"h {x}"));

(int) -> void h = bus.Handlers[0];   // 先接到一个**显式函数类型**的局部变量
h(9);                                // 再调用
```

> 函数值取出后**不能就地调用**：`bus.Handlers[0](9)` 报 `E0402: unsupported call form`，
> 用 `var` 接也认不出是函数（`E0401: undefined function`）。
> 必须像上面那样先赋给一个写明 `(T) -> R` 类型的局部变量。

**参数类型推断是有限的**：目标类型是具体函数类型时，`x => ...` 可以省略参数类型；
目标类型里还含未定的型参（例如在 `List<T>` 的泛型方法上）时必须写全，
否则形参会被当成 `T` 而报错：

```z42
var pos = list.FindAll((int x) => x > 0);   // ✓ 写出 (int x)
// list.FindAll(x => x > 0);                // ✗ E0402: operator `>` … got `T`
```

**捕获**：lambda 可以引用外层的局部变量：

```z42
int k = 10;
var add = (int x) => x + k;
Apply(5, add);                              // 15
```

> z42 的 `List<T>` 上**没有** `Map` / `Filter`，也没有 LINQ。
> 最接近的投影/筛选入口是 `FindAll` / `Find` / `Exists` / `TrueForAll` 等谓词方法。

## 泛型函数

型参、约束与推断见[泛型方法](generic-methods.md)与[泛型约束](generic-constraints.md)。

## 相关

- [命名实参](named-arguments.md)——按名字传参的完整规则
- [参数修饰符 `ref` / `out` / `in`](parameter-modifiers.md)
- [元组](tuples.md)——多返回值
- [泛型方法](generic-methods.md) / [泛型约束](generic-constraints.md)
- [委托与事件](delegates-events.md)——单播 / 多播委托与 `event`
- [实例构造器与初始化子句](constructors.md)——构造器是另一套规则
- [属性与索引器](properties-indexers.md)——`get` / `set` 访问器也是方法
