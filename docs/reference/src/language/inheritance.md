# 继承与多态

> 对齐：2026-09-22 ｜ 实测基准：`./.z42/z42 run`

`class` 之间是**单继承**：一个类至多一个基类，另可实现任意多个[接口](interfaces.md)。
每个 `class` 隐式继承 [`Std.Object`](classes.md#继承与-stdobject)；`struct` **不**参与继承。

```z42
class Animal {
    public string Name { get; set; }
    public Animal(string name) { Name = name; }
    public virtual string Speak() { return "..."; }
}

class Dog : Animal {
    public Dog(string name) : base(name) { }
    public override string Speak() { return "汪"; }
}
```

## `virtual` / `override`

| 修饰符 | 用在 | 含义 |
|--------|------|------|
| `virtual` | 基类方法 | 允许子类改写；调用按**运行时**类型派发 |
| `override` | 子类方法 | 改写基类的 `virtual` / `abstract` 方法 |
| `abstract` | 类 / 方法 | 方法只有签名没有体，由子类提供 |
| `sealed` | 类 / `override` 方法 | 禁止继续继承 / 继续改写，见 [sealed](sealed.md) |

- **不写 `virtual` 的方法不可被改写**——子类声明同名方法不构成多态，且 `override` 一个非
  `virtual` 方法报 `E0429`。
- `override` 的**形参类型必须与被改写者完全一致**；基类另有同 arity 的重载时，`override`
  正确接管对应那一个虚表槽位，见[函数与方法](functions.md#virtual--override)。
- `sealed` 的语义与发射细节（去虚化目标解析）见 [sealed 修饰符](sealed.md)。

## 构造链与 `base`

子类构造器用 `: base(...)` 指定要调用的基类构造器；不写时调用基类的无参构造器
（基类没有无参构造器则报错）。构造顺序是**先基后派生**。

```z42
class Dog : Animal {
    public Dog(string name) : base(name) { }
}
```

`: base(...)` 与 `: this(...)`（转调同类另一个构造器，见[构造器](constructors.md)）
**二选一**，不能同时出现。

## `abstract`

`abstract` 方法只有签名、没有体；含 `abstract` 成员的类必须自己标 `abstract`。
子类要么实现全部 `abstract` 成员，要么自己也标 `abstract`。

```z42
abstract class Shape {
    public abstract double Area();
    public string Describe() { return "面积 " + this.Area().ToString(); }
}

class Rect : Shape {
    private double _w; private double _h;
    public Rect(double w, double h) { _w = w; _h = h; }
    public override double Area() { return _w * _h; }
}
```

`Describe` 是普通方法却能调 `Area()`——这正是抽象方法的用途：**基类定义流程，子类填细节**。

## 调用同类的实例方法

类体内调用自己的实例方法，**裸名与 `this.` 等价**：

```z42
class A {
    public int M() { return 1; }
    public int Bare() { return M(); }        // ≡ this.M()
}
```

静态方法体里没有 `this`，此时裸名调实例方法报 `E0401`（这是正确的——静态上下文本就没有
实例可调）。

> 2026-09-22 之前裸名只认**静态**方法，裸名调实例方法一律报
> `E0401: undefined function`，必须写 `this.M()`——而裸名读字段、读属性、调静态方法三样
> 都是通的，唯独这一项不通。现已对齐。

## 与 C# 的对照

| C# | z42 |
|----|-----|
| 单继承 + 多接口 | ✓ 相同 |
| `virtual` / `override` / `abstract` / `sealed` | ✓ 相同 |
| `: base(...)` / `: this(...)` | ✓ 相同 |
| `new` 方法隐藏（method hiding） | ✗ **不支持**——没有这个语义 |
| `protected` | 见[访问权限控制](access-control.md) |

## 相关

- [类](classes.md)——类定义、成员种类、`Std.Object` 协议
- [sealed 修饰符](sealed.md)——禁止继承 / 禁止继续改写，含去虚化
- [接口](interfaces.md)——另一条抽象途径
- [构造器](constructors.md)——`: this(...)` 与初始化子句
- [函数与方法](functions.md)——重载决议、`override` 与重载的相互作用
