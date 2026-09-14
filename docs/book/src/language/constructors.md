# 实例构造器与初始化子句

> 对齐日期：2026-09-15 · change `add-implicit-base-ctor-call`（前序 `fix-ctor-init-silent-bugs`）

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

## 初始化子句与隐式 `base()`

- `: base(args)` 调基类构造器，`: this(args)` 委托本类另一个构造器。目标的选择与实参处理（按类型的重载决议、
  命名实参、默认值、`params`）与 `new C(args)` 完全相同。
- **实参可以为零个**：`: base()` / `: this()` 与带实参的写法一样生效。
- **没写初始化子句的实例构造器 ⇒ 隐式 `: base()`**（对齐 C#）：
  - 基类没有任何实例构造器 ⇒ 没有可调用的目标，不产生调用；
  - 基类有可以零实参调用的构造器（无参 / 形参全带默认值 / 只有 `params` 尾参）⇒ 调用它；
  - 基类有构造器、但无一可以零实参调用 ⇒ **E0469**，须显式写 `: base(...)`。
- `: this(..)` 委托的构造器不隐式调用基类——由被委托的那个构造器调用，基类构造器只执行一次。

```z42
class B { public B(int x) { } }
class D : B {
    public D() { }            // ❌ E0469：B 没有无参构造器，须写 `: base(...)`
    public D(int x) : base(x) { }
}
```

## 构造器继承

**没写任何实例构造器的类，继承基类全部非 private 的实例构造器。** 每个继承来的构造器的形参与基类那个相同
（含默认值与 `params`；泛型基类按类型实参代换），执行「本类初始化器 → `base(同样的实参)`」。继承是传递的。

```z42
class MyErr : Exception { }        // 什么都不用写
throw new MyErr("bad");             // 继承 Exception(string)

class Box<T> { public T v; public Box(T x) { this.v = x; } }
class IntBox : Box<int> { public int hits = 0; }
new IntBox(5);                      // 继承 Box(T)，T 代换为 int；hits 初始化器照常执行
```

- 基类没有可继承的构造器时，本类若有实例初始化器，编译器合成 `public C() { }`（执行初始化器）。
- **写了任何一个实例构造器，就不再继承**——此时只有自己写的那些（与 C# 一致），没写 `: base(...)` 的按上面的
  隐式 `base()` 规则。给派生类加第一个构造器会让继承来的构造器消失，调用点会**编译报错**，不会静默改变行为。
- 继承来的构造器与合成的默认构造器是普通的构造器：参与重载决议、E0426、`where T : new()`，随包导出，
  跨包的派生类同样可以继承和调用。
- 暂不支持「既写自己的构造器、又保留继承来的」（C++ 的 `using Base::Base;`）。

### 为什么这样设计

C# 要求派生类把基类每个构造器手写一遍转发（`MyErr(string m) : base(m) { }`），绝大多数没有任何自定义逻辑。
Kotlin / Dart 只是把转发写得更短；C++11 要显式写 `using Base::Base;`。z42 采用 Swift 的规则：派生类
**没说要怎么构造，就按基类的方式构造**。Swift 要求新增属性都有默认值，而 z42 的字段总有默认值（无初始化器即
类型零值），所以这条规则可以无条件成立。

## 可见性

构造器与字段、方法遵守同一套可见性规则（见 [访问权限强制](../compiler/access-control.md)）：

| 写法 | 谁能调用（`new` / `: base(..)` / `: this(..)`） |
|---|---|
| `public C(..)` | 任何地方 |
| `protected C(..)` | 本类与派生类（派生类的 `: base(..)`；外部 `new` 不行） |
| `internal C(..)` | 同一个包 |
| `private C(..)`，或**不写修饰符** | 只有本类（静态工厂、`: this(..)`） |
| 主构造器 `class P(int X)` | 任何地方（public，对齐 C#） |

```z42
class C { C(int a) { } }                  // 不写修饰符 = private
var c = new C(1);                          // ❌ E0404：cannot access private constructor `C` of `C`

class Token {
    private Token(string s) { }
    public static Token Parse(string s) { return new Token(s); }   // ✅ 本类内
}
```

## 静态构造器不是实例构造器

同一个类可以同时有 `static C() { }` 与 `C() { }`。静态构造器只作为类型初始化器执行（见
[静态构造函数](static-constructors.md)），**不参与**任何实例构造：

- `new C(..)` 与 `: base(..)` / `: this(..)` 的构造器选择只看实例构造器；
- 实参个数校验（E0426）只看实例构造器——只有 `static C()` 与 `C(int)` 时 `new C()` 报 E0426；
- `where T : new()` 只看实例构造器——只写了静态构造器的类等同于「没有显式构造器」，满足约束。

## 历史

2026-09-14 之前有两个静默错误（不报错、字段停在默认值）：零实参的 `: base()` / `: this()` 被整条丢弃；
同一个类里的静态构造器被当作无参实例构造器选中，导致 `new C()` 不执行实例构造器，并让 E0426 与
`new()` 约束检查把静态构造器算作「可零实参调用」。回归用例：`src/tests/classes/ctor_init_clauses.z42`、
`src/tests/classes/static_ctor_with_instance_ctor.z42`。

2026-09-15 之前 z42 **不自动调用基类构造器**，而「无显式构造器的类」只内联**同一编译单元**里祖先的字段初始化器：
同包跨文件、跨包的基类初始化器静默丢失；`class D : W { }` 连基类构造器也不执行；派生类也无法继承基类构造器。
回归用例：`src/tests/classes/implicit_base_ctor.z42`、`src/tests/classes/inherited_ctors.z42`、
`src/tests/cross-zpkg/inherited_ctor_cross_pkg/`。实现见 [构造器继承与隐式 base()](../compiler/ctor-inheritance.md)。
