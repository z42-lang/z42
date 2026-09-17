# 接口

`interface` 声明一组成员**签名**，由实现它的类型提供实现。
一个类只能有一个基类，但可以实现任意多个接口；`struct` 也可以实现接口。

## 声明与实现

```z42
using Std.IO;

public interface IShape {
    double Area();
    string Name { get; }         // 接口属性
}

public interface IDrawable {
    void Draw();
}

// 多接口实现：逗号分隔
public class Circle : IShape, IDrawable {
    public double Radius { get; }

    public Circle(double radius) { Radius = radius; }

    public double Area() => 3.14159 * Radius * Radius;
    public string Name       => "Circle";
    public void Draw() { Console.WriteLine($"Drawing {Name} r={Radius}"); }
}

void Main() {
    IShape s = new Circle(2.0);
    Console.WriteLine(s.Name);                  // Circle
    Console.WriteLine(s.Area().ToString());     // 12.56636

    IDrawable d = new Circle(1.0);
    d.Draw();                                   // Drawing Circle r=1
}
```

接口成员**不写访问修饰符**即可（写 `public` 也接受），没有函数体。
实现方的成员必须是 `public`。

有基类又有接口时，基类写在最前面：`class C : Base, IFoo, IBar`。

## 满足性校验

实现类少写一个接口成员报 **`E0412`**：

```z42
interface IShape { double Area(); }
class Bad : IShape { }
// ✗ E0412: `Bad` implements `IShape` but does not define member `Area`
```

签名不一致（参数类型 / 个数对不上）同样报 `E0412`。
实现成员**必须是 `public`**，否则：

```z42
class C : IA { int A() { return 3; } }
// ✗ E0412: `C` implements `IA` but `A` must be implemented `public` (it is `private` here)
```

## 可以出现在接口里的成员

### 方法与属性

```z42
interface IPipelineContext {
    string Name { get; }
    void Run(string stage);
}
```

属性的写法与规则见[属性与索引器](properties-indexers.md)。

### 索引器

```z42
interface IBox {
    int this[int i] { get; set; }
}

class ArrBox : IBox {
    int[] data;
    public ArrBox(int[] d) { this.data = d; }
    public int this[int i] {
        get { return this.data[i]; }
        set { this.data[i] = value; }
    }
}

IBox b = new ArrBox(new int[] { 1, 2, 3 });
b[1];             // 2 —— 经接口静态类型读
b[1] = 99;        // 经接口静态类型写
```

### 事件

```z42
interface IBus {
    event MulticastAction<int> Clicked;
}

class Bus : IBus {
    public event MulticastAction<int> Clicked;
    public void Fire(int x) { this.Clicked.Invoke(x); }
}

Bus bus = new Bus();
IBus iface = bus;
iface.Clicked += (int x) => Console.WriteLine($"click {x}");   // 经接口订阅
bus.Fire(7);                                                   // click 7
```

`+=` / `-=` 在接口引用上同样脱糖成 `add_Clicked` / `remove_Clicked`，按虚表派发到实现类。
事件与多播委托的完整语义见[委托与事件](delegates-events.md)。

## 泛型接口

```z42
interface IRepo<T> {
    T Get(int id);
    void Put(T item);
}

class Box : IRepo<string> {
    public string Get(int id) { return "g" + id.ToString(); }
    public void Put(string item) { Console.WriteLine("put " + item); }
}
```

接口作为**泛型约束**（`where T : IShape`）的规则见[泛型约束](generic-constraints.md)。

## `Self` 类型

接口里可以写 `Self`，指代**实现该接口的那个类型**——省掉 `IEq<T> where T : IEq<T>`
这种自引用样板：

```z42
interface ICloneable { Self Copy(); }

class Tag : ICloneable {
    public string Text;
    public Tag(string t) { Text = t; }
    public Tag Copy() { return new Tag(this.Text); }   // Self 落地成 Tag
}

Tag t = new Tag("x");
Tag t2 = t.Copy();                 // 在具体类上调用 ⇒ 结果类型 Tag

ICloneable c = new Tag("hi");
var copy = c.Copy();               // 经接口静态类型调用 ⇒ 结果类型 ICloneable
```

两条要记住的边界：

- `Self` **只能写在接口里**。类里写 `Self` 报 `E0443: undefined type: Self`。
- 经**接口静态类型**调用返回 `Self` 的方法，结果类型就是**该接口本身**（可靠的上界，但不是具体类型）。
  上例的 `copy.ToString()` 会因 `ICloneable` 上没有这个成员而报 `E0401`。

标准库的 `IEquatable` / `IComparable` / `INumber` 都是**非泛型 + `Self`** 的形态。
完整规则、实现模型与 `E0463` 见[泛型约束的 `Self` 类型](generic-constraints.md)。

## `static abstract` 成员（部分实现）

接口可以声明 `static abstract` 成员，实现方用 `static override` 提供。
目前**可靠可用的是标准库的 `Std.INumber`**——它把 5 个二元算术运算符声明成 `static abstract`，
让泛型代码在型参上直接写 `a + b`：

```z42
using Std;

struct Money : INumber {
    public long Cents;
    public Money(long c) { this.Cents = c; }
    public static override Money op_Add(Money a, Money b)      { return new Money(a.Cents + b.Cents); }
    public static override Money op_Subtract(Money a, Money b) { return new Money(a.Cents - b.Cents); }
    public static override Money op_Multiply(Money a, Money b) { return new Money(a.Cents * b.Cents); }
    public static override Money op_Divide(Money a, Money b)   { return new Money(a.Cents / b.Cents); }
    public static override Money op_Modulo(Money a, Money b)   { return new Money(a.Cents % b.Cents); }
}

T Sum3<T>(T a, T b, T c) where T : INumber { return a + b + c; }

Sum3(new Money(1), new Money(2), new Money(3)).Cents;   // 6
Sum3(1, 2, 3);                                          // 6 —— 基元类型同样实现了 INumber
```

实现方必须实现 `INumber` 的**全部 5 个**运算符，否则报 `E0412`。

> ### ⚠️ 这只是一个子集
>
> 派发是**由接收者的值驱动**的——运行期看实参的具体类型选实现。因此：
>
> - **没有值可派发的形态用不了**：`where T : IZero` 下写 `T.Zero()` 报
>   `E0401: undefined: T`。
> - **自定义的 `static abstract` 接口目前不可靠**：同样形状的用户接口
>   （`interface ICombine { static abstract Self op_Add(Self a, Self b); }`）
>   实测在运行期抛 `MissingSymbolException`。
>
> 需要这类能力时，请实现标准库的 `Std.INumber`，而不是自己声明 `static abstract` 接口。

## 已知缺口：接口不能继承接口

`interface IDerived : IBase { ... }` 能通过语法解析，但**继承不生效**：

```z42
interface IBase { int Base(); }
interface IDerived : IBase { int Extra(); }
class Impl : IDerived {
    public int Base() { return 1; }
    public int Extra() { return 2; }
}

IDerived d = new Impl();
d.Extra();        // ✓
d.Base();         // ✗ E0401: no method `Base` on interface `IDerived`
IBase b = d;      // ✗ E0402: cannot assign IDerived to IBase
```

目前请让实现类**直接列出所有接口**（`class Impl : IBase, IDerived`），
并在需要 `IBase` 视图时用具体类型赋值。

## 已知缺口：没有默认实现

接口方法写了函数体也**不构成默认实现**——实现类仍必须自己定义该成员：

```z42
interface IGreet {
    string Name();
    string Hello() { return "hi " + Name(); }   // 函数体被解析，但不生效
}
class G : IGreet { public string Name() { return "z"; } }
// ✗ E0412: `G` implements `IGreet` but does not define member `Hello`
```

需要共享实现时，改用抽象基类，或把逻辑放进一个静态辅助类。

## 相关

- [类](classes.md) / [结构体](structs.md)——两种实现方
- [属性与索引器](properties-indexers.md)——接口属性与接口索引器
- [委托与事件](delegates-events.md)——接口 `event`
- [泛型约束](generic-constraints.md)——`where T : IFoo`、`Self`、型参上的运算符派发
- [模式匹配](pattern-matching.md)——`is` / `switch` 里的类型模式
- [`[Forward]`（成员转发）](member-forwarding.md)——把接口面转发给内部字段
