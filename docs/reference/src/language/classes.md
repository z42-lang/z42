# 类

`class` 定义**引用类型**：变量持有的是对象的引用，赋值复制引用而不是内容。
值类型见[结构体](structs.md)，值/引用语义的整体规则见[所有权与内存模型](memory-model.md)。

## 类定义

```z42
namespace Geometry;

using Std;
using Std.IO;

public class Point {
    // 自动属性
    public double X { get; set; }
    public double Y { get; set; }

    // 构造器
    public Point(double x, double y) { X = x; Y = y; }

    // 实例方法
    public double DistanceTo(Point other) {
        double dx = X - other.X;
        double dy = Y - other.Y;
        return Math.Sqrt(dx * dx + dy * dy);
    }

    // 重写 Object.ToString
    public override string ToString() => $"Point({X}, {Y})";

    // 静态工厂
    public static Point Origin() => new Point(0.0, 0.0);
}

// 继承：单基类
public class Point3D : Point {
    public double Z { get; set; }
    public Point3D(double x, double y, double z) : base(x, y) { Z = z; }
    public override string ToString() => $"Point3D({X}, {Y}, {Z})";
}
```

可写的成员种类：字段、[属性与索引器](properties-indexers.md)、方法、
[构造器](constructors.md)、[静态构造器](static-constructors.md)、
[`const`](const.md) 与 [`readonly`](readonly-fields.md) 字段、
[事件](delegates-events.md)、[嵌套类型](nested-types.md)。
可见性修饰符见[访问权限控制](access-control.md)；
`sealed`、`static`、`partial` 分别见 [sealed](sealed.md)、[static 类](static-classes.md)、
[partial 类型](partial-types.md)。

## 继承与 `Std.Object`

每个 `class` 都**隐式继承 `Std.Object`**，并且只能有一个基类（可以另外实现任意多个
[接口](interfaces.md)）。`struct` **不**继承 `Object`——编译器直接为值类型合成值语义的
`Equals` / `GetHashCode` / `ToString`，不走继承和虚表。

`Object` 提供这些成员：

| 成员 | 签名 | 行为 |
|---|---|---|
| `GetType()` | `Type GetType()` | 返回运行时 `Type` 描述符 |
| `Equals` | `virtual bool Equals(Object? other)` | 默认引用相等；子类可重写为值相等 |
| `GetHashCode` | `virtual int GetHashCode()` | 默认基于对象身份；重写 `Equals` 时必须同步重写 |
| `ToString` | `virtual string ToString()` | 默认返回**不含命名空间**的类名；通常应重写 |
| `ReferenceEquals` | `static bool ReferenceEquals(Object? a, Object? b)` | 堆地址相等（两个 `null` 也为 `true`） |

> `Object.ReferenceEquals` **目前从用户代码调用不到**——`Object.ReferenceEquals(a, b)` 与
> `Std.Object.ReferenceEquals(a, b)` 都报 `E0401: no static method 'ReferenceEquals' on 'Object'`。
> 比较委托的引用身份请用 `DelegateOps.ReferenceEquals`（见[委托与事件](delegates-events.md)）。

规则：

- 重写 `Equals` 时必须同时重写 `GetHashCode`，两者必须保持一致。
- `ReferenceEquals` 是静态方法，不可被重写。
- `ToString()` 默认只给短类名；要完全限定名用 `GetType().FullName`。

### `Type` 描述符

`Type` 是轻量的运行时类型描述符，只能通过 `GetType()` 拿到，不能直接构造：

```z42
var t = new Point3D(1.0, 2.0, 3.0).GetType();
Console.WriteLine(t.Name);      // Point3D
Console.WriteLine(t.FullName);  // Geometry.Point3D
```

`FullName` **原样保留命名空间的大小写**。

## 构造器

一个类可以有多个构造器（按参数类型重载），用 `: base(...)` / `: this(...)` 初始化子句委托。
没写初始化子句的实例构造器隐式 `: base()`——基类没有无参构造器时报 `E0469`。
完整规则（执行顺序、构造器继承、合成默认构造器、可见性）见
[实例构造器与初始化子句](constructors.md)。

### 字段默认值

实例**字段**（不是自动属性）可以在声明处写 `=` 初始化器；没写就取类型默认值：

```z42
class Box {
    public int N = 5;
    public string S = "hello";
    public bool Flag;        // false
    public Point P;          // null
}
```

| 字段写法 | 无显式构造器时的初值 | 有显式构造器时 |
|---|---|---|
| `int n;` | `0` | 构造器体内最后一次赋值 |
| `int n = 5;` | `5` | 先注入 `5`，再被构造器体的赋值覆写 |
| `bool flag;` | `false` | 同上 |
| `string s;` | `null` | 同上 |
| `string s = "x";` | `"x"` | 同上 |
| `Point p;` | `null` | 同上 |

字段初始化器按声明顺序在构造器最前面执行，**早于** `: base(...)`、更早于构造器体；
写了 `: this(...)` 的构造器会跳过这一步（由被委托的那个构造器执行，保证只跑一次）。
完整执行顺序见[实例构造器与初始化子句](constructors.md)。

## 对象初始化器

`new Type(args?) { ... }` 在构造之后逐字段赋值；裸标识符是字段简写（`x` ≡ `x = x`）：

```z42
var p = new Point { X = 1.0, Y = 2.0 };      // 显式字段

double X = 3.0;
double Y = 4.0;
var q = new Point { X, Y };                  // 简写：X ≡ X = X（需作用域里有同名变量）

var b = new Box(5, 6) { Filled = true };     // 带构造实参
```

完整规则见[对象初始化器](object-initializers.md)；省略类名的 `new()` 见
[target-typed new](target-typed-new.md)。

## 主构造器

类名后直接跟参数表即是**主构造器**：参数成为该类的字段，构造器由编译器合成。

```z42
class Counter(int start) {
    public int Next() => start + 1;      // 裸 start 解析为字段
}

var c = new Counter(41);
c.Next();                                // 42
```

不带 `[Record]` 时，主构造器参数生成的字段是 **private**；带 `[Record]` 时是 **public**，
另外还会拿到值相等、记录式 `ToString`、`with` 与位置解构。见
[`[Record]` attribute 与主构造器](record-attribute.md)。

> ### ⚠️ 主构造器不能向基类转发实参
>
> C# 12 的 `class Point3D(double X, double Y, double Z) : Point(X, Y)` 在 z42 **不成立**——
> 基类列表只接受**类型名**，不接受实参表，写了会在 `(` 处直接报语法错误（`E0202`），
> 接着因为合成的主构造器没有调用基类构造器而报 `E0469`。
>
> 需要向基类传参时改写显式构造器：
>
> ```z42
> public class Point3D : Point {
>     public double Z { get; set; }
>     public Point3D(double x, double y, double z) : base(x, y) { Z = z; }
> }
> ```

## 相关

- [结构体](structs.md)——值类型的对应物
- [接口](interfaces.md)——类可以实现任意多个接口
- [实例构造器与初始化子句](constructors.md) / [静态构造函数](static-constructors.md)
- [属性与索引器](properties-indexers.md) / [readonly 字段](readonly-fields.md) / [const](const.md)
- [`[Record]` attribute 与主构造器](record-attribute.md)
- [访问权限控制](access-control.md) / [sealed](sealed.md) / [static 类](static-classes.md) / [partial 类型](partial-types.md) / [嵌套类型](nested-types.md)
- [所有权与内存模型](memory-model.md)——引用语义、复制发生在哪里、装箱
- [模式匹配](pattern-matching.md)——封闭类层次与穷尽性检查
