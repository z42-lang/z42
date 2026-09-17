# 结构体

`struct` 定义**值类型**：赋值、传参、存进字段都是**复制内容**而不是共享引用。

本页讲 struct 怎么定义、能写什么、有哪些限制。
「哪些类型复制值、哪些复制引用、`ref`/`out`/`in` 与装箱」这些跨类型的语义规则在
[所有权与内存模型](memory-model.md)。

## 定义

```z42
using Std.IO;

public struct Color {
    public byte R;
    public byte G;
    public byte B;

    public Color(byte r, byte g, byte b) { R = r; G = g; B = b; }

    public int ToRgb() => (R << 16) | (G << 8) | B;

    public override string ToString() => $"#{R}-{G}-{B}";
}

void Main() {
    var red = new Color(255, 0, 0);
    var copy = red;                   // 值拷贝
    copy.G = 128;
    Console.WriteLine($"{red.G} {copy.G}");        // 0 128 —— 改副本不影响原件
    Console.WriteLine(red.ToRgb().ToString());     // 16711680
}
```

struct 可以有字段、方法、构造器、`static` 方法、运算符重载，也可以
[实现接口](interfaces.md)、带 [`[Record]`](record-attribute.md)。

## 值语义由编译器合成

`struct` **不**继承 `Std.Object`。编译器直接为值类型合成 `Equals` / `GetHashCode`，
按**字段内容**比较：

```z42
struct Pair { public int A; public int B; public Pair(int a, int b) { A = a; B = b; } }

var a = new Pair(1, 2);
var b = new Pair(1, 2);
var c = new Pair(9, 9);

a.Equals(b);                          // true —— 值相等
a.Equals(c);                          // false
a.GetHashCode() == b.GetHashCode();   // true
```

`ToString()` 是个例外：普通 struct 没有自定义 `ToString` 时只返回**短类型名**（`"Pair"`）。
想要「显示内容」两条路：

- 自己写 `public override string ToString() => ...`；
- 或者用 [`[Record]` struct](record-attribute.md)，它合成记录式的输出：

```z42
[Record] struct Vector2(double X, double Y);

new Vector2(1.5, 2.5).ToString();     // Vector2 { X = 1.5, Y = 2.5 }
```

[元组](tuples.md) `(a, b)` 就是编译器合成的值元组 struct，语义与手写 struct 一致。

## 限制与已知缺口

### struct 不能继承

struct 不参与继承层次，没有基类也不能被继承。

> ⚠️ 给 struct 写基类列表（`struct B : A`，`A` 也是 struct）目前**不报错**，
> 但既不会继承字段、访问时也会在运行期崩。不要这么写。
> 写接口名是可以的（见下）。

### 自动属性在 struct 上不可用

```z42
public struct P {
    public int X { get; set; }        // ⚠️ 编译通过，构造时运行期崩
    public P(int x) { X = x; }
}
```

实测报 `struct ref leaf at byte offset ... not in type layout`。
**struct 请用公开字段**；需要计算属性时写方法。

### 表达式体构造器 + 元组赋值会静默失效

```z42
public Pair(int a, int b) => (A, B) = (a, b);   // ⚠️ 编译通过，字段全是 0
```

`(A, B) = (a, b)` 是**解构声明**——它声明两个新的局部变量 `A` / `B`，
不是给已有字段赋值。struct 构造器请写花括号体：

```z42
public Pair(int a, int b) { A = a; B = b; }
```

### `static` 字段不能持有 struct 值

```z42
public static readonly Color White = new Color(255, 255, 255);   // ⚠️ 读取时运行期崩
```

无论加不加 `readonly`，struct 类型的静态字段在读取时都会抛
`struct-value handle used after its creating frame exited`。
用**静态工厂方法**代替：

```z42
public static Color White() => new Color(255, 255, 255);

Color.White().ToRgb();      // ✓ 16777215
```

### `default(T)` 对 struct 产出 `null`

```z42
var p = default(Pair);
p.Sum();     // ⚠️ 运行期抛：StructCopy src: expected a struct value (StructRef), got Null
```

这是已知缺陷，不是设计语义。需要「零值」时显式 `new Pair(0, 0)`。

### 单字段 struct 仍表现为引用语义

值语义目前只对**两个及以上字段**的 struct 生效。
细节与示例见[所有权与内存模型的「已知偏差」](memory-model.md)。

## 与接口一起用

struct 可以实现接口；经接口静态类型调用会装箱：

```z42
interface IArea { int Area(); }

struct Rect : IArea {
    public int W;
    public int H;
    public Rect(int w, int h) { W = w; H = h; }
    public int Area() { return W * H; }
}

var r = new Rect(3, 4);
r.Area();                  // 12（直接调用，不装箱）
IArea a = r;               // 装箱
a.Area();                  // 12
```

struct 也可以实现带 `static abstract` 成员的接口（如 `Std.INumber`），
从而在泛型约束下参与运算符派发——见[接口](interfaces.md)与[泛型约束](generic-constraints.md)。

## 相关

- [所有权与内存模型](memory-model.md)——值/引用语义、复制点、`ref`/`out`/`in`、装箱
- [元组](tuples.md)——合成的值元组 struct
- [`[Record]` attribute 与主构造器](record-attribute.md)——`[Record] struct`
- [类](classes.md)——引用类型的对应物
- [接口](interfaces.md) / [泛型约束](generic-constraints.md)
