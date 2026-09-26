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

## 默认值

`default(T)` 产出一个**所有字段都是零值**的 struct —— 基元叶子为 `0` / `false` / `'\0'`，
引用叶子为 `null`。等价于无参 `new T()`：

```z42
struct Pt { public int X; public int Y; public string Tag; }

var p = default(Pt);
p.X;      // 0
p.Tag;    // null
```

每次求值都产出**独立**的一份，不是共享的单例。

## 静态字段可以持有 struct 值

```z42
struct Color {
    public int R; public int G; public int B;
    public Color(int r, int g, int b) { R = r; G = g; B = b; }

    public static readonly Color White = new Color(255, 255, 255);
}

Color.White.R;        // 255
```

带不带 `readonly` 都可以，声明在 struct 自身或别的类里都可以。

读取静态 struct 字段拿到的是**独立副本**，改副本不影响静态字段本身；
而**就地**改静态字段的某个成员（`Color.White.R = 0`）是持久的：

```z42
var c = Color.White;
c.R = 0;                  // 只改副本
Color.White.R;            // 仍是 255

Color.White.R = 0;        // 就地改
Color.White.R;            // 0
```

## 属性

struct 支持**自动属性**与**计算属性**，写法与类一致：

```z42
struct Rect {
    public int W { get; set; }        // 自动属性：编译器合成后备存储
    public int H { get; set; }
    public Rect(int w, int h) { W = w; H = h; }

    public int Area { get { return W * H; } }   // 计算属性：无存储
}

var r = new Rect(3, 4);
r.W;         // 3
r.W = 5;
r.Area;      // 20
```

自动属性的后备存储占 struct 布局里的一格，与普通字段并列，因此
「自动属性 + 普通字段」混用时各自的偏移互不影响。计算属性不占存储。

> 自动属性不改变值语义：`var b = a;` 仍是整份 blob 复制，改 `b.W` 不影响 `a.W`。

## 限制与已知缺口

### struct 不能继承

struct 不参与继承层次，没有基类也不能被继承。

> ⚠️ 给 struct 写基类列表（`struct B : A`，`A` 也是 struct）目前**不报错**，
> 但既不会继承字段、访问时也会在运行期崩。不要这么写。
> 写接口名是可以的（见下）。

### 表达式体构造器 + 元组赋值会静默失效

```z42
public Pair(int a, int b) => (A, B) = (a, b);   // ⚠️ 编译通过，字段全是 0
```

`(A, B) = (a, b)` 是**解构声明**——它声明两个新的局部变量 `A` / `B`，
不是给已有字段赋值。struct 构造器请写花括号体：

```z42
public Pair(int a, int b) { A = a; B = b; }
```

### 值语义与字段数无关

一个字段的 struct 与多字段 struct 走**同一个**值模型（字节 blob + 逐叶子复制）：赋值、传参、
返回、数组元素、类的内联字段一律是复制。

> 📜 **2026-09-26 之前（`single-field-struct-value-semantics`）**：闸门 `IsBlobStruct` 要求
> `FieldCount >= 2`，**单字段 struct 落在引用模型上** —— `S b = a; b.X = 50;` 会改到 `a`。
> 翻闸门要**编译器与 VM 两侧同时改**：VM 在 `try_struct_backed` 有一份逐字镜像的判据，
> 只翻一侧会让 `S[]`（单字段）退化成引用数组、元素全 `Null`。
> 同一刀还连带修了两条既存缺陷：跨包静态调用漏传 sret（`fix-crosspkg-static-sret`）、
> `extern` 桩不支持 blob 返回（`GCHandle.Alloc`）。

### `ToString` 在所有字符串化路径上一致

自定义的 `ToString` 在**每一条**字符串化路径上生效，答案完全相同：

| 写法 | 结果 |
|------|------|
| `s.ToString()` | ✅ 自定义结果 |
| `$"{s}"` 插值 | ✅ 自定义结果 |
| `"x" + s` 拼接 | ✅ 自定义结果 |
| `((object)s).ToString()` | ✅ 自定义结果 |
| `Console.WriteLine(s)` / `Write(s)` | ✅ 自定义结果 |

**没有**自声明 `ToString` 的类型，五条路一律给**短类型名**（`Point`，不是 `Point{...}`）；
`[Record]` 的合成 `ToString` 给 `Point { X = 1, Y = 2 }`；enum 给成员名。

⚠️ **自指的 `ToString` 会无限递归**：`public override string ToString() => "P" + this;`
—— 拼接会再次调用 `ToString`，栈溢出（C# 同）。要打类型名用 `GetType().Name`。

> **收敛过程**（两刀）：
> - 2026-09-22 `fix-struct-tostring-paths` 修插值 / 拼接 / 经 `object` 调用
>   （此前插值与拼接吐 `<struct value>` 占位符）——但**只覆盖两个及以上字段的 struct**。
> - 2026-09-25 `dispatch-tostring-in-native-stringify` 补齐
>   `Console.WriteLine` / `Write`（对所有类型），以及**拼接**对 **class / record /
>   单字段 struct** ——本页此前把 `"x" + s` 一律记作 ✅，那**只对双字段 struct 成立**。
>   机制：`builtin_println` 与 `exec_value::add` 改走 `obj_to_string`
>   （它一直在重入 VM 派发，只是这两条路没接上它）。

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
