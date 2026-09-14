# 实例构造器与初始化子句

> 对齐日期：2026-09-14 · change `fix-ctor-init-silent-bugs`

```z42
class Shape {
    public string name;
    public Shape() { this.name = "shape"; }
    public Shape(string n) { this.name = n; }
}

class Circle : Shape {
    public double r = 1.0;                        // 字段初始化器
    public Circle() : base() { }                  // 调基类无参 ctor
    public Circle(double r) : this() { this.r = r; }   // 委托本类无参 ctor
    public Circle(string n, double r) : base(n) { this.r = r; }
}
```

## 执行顺序

`new C(..)` 选中的构造器按下面顺序执行（对标 C#）：

1. **本类**的实例字段 / auto 属性初始化器，按声明顺序；
2. 初始化子句的目标构造器（`: base(..)` 或 `: this(..)`）；
3. 本构造器的体。

`: this(..)` 委托时第 1 步**跳过**——初始化器由被委托的那个构造器执行，保证只跑一次。

```z42
class L { public static string T = ""; public static int M(int v) { L.T = L.T + v.ToString(); return v; } }
class P { public P(int v) { L.M(v); } }
class Q : P { public int f = L.M(1); public Q() : base(2) { L.M(3); } }
// new Q() ⇒ L.T == "123"
```

## 初始化子句

- `: base(args)` 调基类构造器，`: this(args)` 委托本类另一个构造器；实参按普通重载决议选目标。
- **实参可以为零个**：`: base()` / `: this()` 与带实参的写法完全一样生效。
- z42 **不会自动调用**基类构造器：没写 `: base(..)` 的显式构造器不会执行基类构造器的体。
- `: base()` 指向一个**没有显式实例构造器**的基类时，没有可调用的目标，子句不产生调用。

## 静态构造器不是实例构造器

同一个类可以同时有 `static C() { }` 与 `C() { }`。静态构造器只作为类型初始化器执行（见
[静态构造函数](static-constructors.md)），**不参与**任何实例构造：

- `new C(..)` 与 `: base(..)` / `: this(..)` 的构造器选择只看实例构造器；
- 实参个数校验（E0426）只看实例构造器——只有 `static C()` 与 `C(int)` 时 `new C()` 报 E0426；
- `where T : new()` 只看实例构造器——只写了静态构造器的类等同于「没有显式构造器」，满足约束。

## ⚠️ 已知缺口

基类**只有字段初始化器、没有显式构造器**，而派生类写了显式构造器时，基类的字段初始化器不会执行：

```z42
class B { public int b = 7; }
class D : B { public D(int x) { } }
// new D(3).b == 0（C# 为 7）
```

派生类没有显式构造器时（编译器合成构造器）会内联整条祖先链的初始化器，不受影响。这与「不自动调用
基类构造器」的模型相关，修正方式属于语义决策，尚未立项。

## 历史

2026-09-14 之前有两个静默错误（不报错、字段停在默认值）：零实参的 `: base()` / `: this()` 被整条丢弃；
同一个类里的静态构造器被当作无参实例构造器选中，导致 `new C()` 不执行实例构造器，并让 E0426 与
`new()` 约束检查把静态构造器算作「可零实参调用」。回归用例：`src/tests/classes/ctor_init_clauses.z42`、
`src/tests/classes/static_ctor_with_instance_ctor.z42`。
