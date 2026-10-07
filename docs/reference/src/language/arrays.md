# 数组

`T[]` 是**一维动态数组**，**引用类型**（堆分配，赋值传引用）。方括号 `[]` 是数组的专属字面量
语法；花括号 `{}` 归 List / Dictionary，见 [集合字面量](collection-literals.md)。

## 语法

### 创建与访问

```z42
int[] arr = new int[] { 1, 2, 3 };   // 字面量初始化
int[] arr2 = new int[n];             // 指定长度，零值初始化
                                     //   int→0, bool→false, string→""/null, object→null
int x = arr[0];                      // 读
arr[1] = 42;                         // 写
int len = arr.Length;                // 长度（int）
```

### 方括号字面量

```z42
// 1. 元素字面量（元素类型来自目标类型；var → 元素公共类型）
int[] xs = [1, 2, 3];               // 等价 new int[]{ 1, 2, 3 }
var   ys = [10, 20, 30];            // → int[]
int[] e  = [];                      // 空数组（元素类型来自目标；var 无目标 → 报错）

// 2. 重复填充 [value; count]（Rust 风）
int[] zeros  = [0; 100];
int[] sevens = [7; n];              // count 可以是运行期值

// 3. spread 展开 [..a, x, ..b]（拼接数组片段 + 散元素）
int[] cat = [..xs, 99, ..ys];       // spread 源目前仅数组
```

规则：

- **`[v; n]` 的 `v` 只求值一次**，结果填进每个槽——`v` 是引用类型时 n 个槽指向**同一个对象**。
- **元素公共类型**：无目标类型时取首元素类型；混合数值（`[1, 2L]`）按目标类型加宽，无目标则以
  首元素为准。
- **空 `[]` 必须有目标元素类型**（`var x = []` 报错）。

脱糖规则与 `{}` 侧并列列在 [集合字面量](collection-literals.md)，此处不重复。

### 交错数组（jagged）`T[][]`

**支持**：类型位任意层 `[]`、`new T[][]{...}`、方括号字面量、逐层下标与逐层 `.Length`。

```z42
int[][] rows = new int[][] { new int[]{1, 2}, new int[]{3, 4, 5} };
int[][] lit  = [ [7, 8], [9] ];
rows[0][1] = 9;
rows.Length;      // 2
rows[1].Length;   // 3
```

两条边界：

- **`new T[n][]` 不解析**（C# 里「按运行期长度分配外层行数组」那种写法）。要按运行期长度建外层，
  用重复填充 `int[][] rows = [null; n];` 再逐行赋值。
- **多维数组 `T[,]` 不支持**——没有这种类型语法；多维下标 `a[i, j]` 报 **E0402**
  （发射点 `src/compiler/z42c.semantics/src/Binding/ExprTyper.z42:151`，诊断直接提示改用 jagged `a[i][j]`）。

## 语义

- **索引越界抛 `IndexOutOfRangeException`**（含负下标），消息形如
  `index 5 is out of range for an array of length 3`，可以按类型 `catch`。
- **数组为 null** 时读写元素、取 `.Length` 抛 `NullReferenceException`；`new T[n]` 的 `n` 为负抛 `OverflowException`。
- **`.Length` 返回 `int`。**
- **元素类型编译期检查**：`arr[i] = v` 中 `v` 必须可赋给元素类型。
- **数组不变（无协变）**：`Dog[]` **不**可赋给 `Animal[]`。

## 运行时类型：`Std.Array`

所有 `T[]` 在运行时是 `Std.Array` 的实例，类型链为 `T[]` is-a `Std.Array` is-a `Std.Object`
（真子类型链，不是对 `object` 的兜底）。

数组在反射里携带**元素类型**，不被擦除：

```z42
typeof(int[]).FullName;                  // "Std.Int32[]"
typeof(int[]).Name;                      // "Int32[]"
typeof(int[]).IsArray;                   // true
typeof(int[]).GetElementType().FullName; // "Std.Int32"
typeof(int[][]).FullName;                // "Std.Int32[][]"（逐层递归）

int[] xs = new int[3];
xs.GetType().FullName;                   // "Std.Int32[]"，与 typeof 一致
```

`Std.Array` 上除 `Length` / `Clone()` 与对象协议（`GetType` / `Equals` / `GetHashCode` /
`ToString`）之外，还提供**一整套静态算法**——`Sort`（4 个重载）、`BinarySearch`（3）、`IndexOf`
（3）、`LastIndexOf`、`Contains`、`Find` / `FindLast` / `FindIndex` / `FindLastIndex` / `FindAll`、
`Exists` / `TrueForAll`、`ConvertAll`、`ForEach`、`Copy` / `CopyRange`、`Fill`、`Reverse`、
`Clear`、`Resize`、`Empty`、`AsReadOnly`、`CreateInstance` / `GetValue` / `SetValue`。逐个签名见
标准库参考的 `Std.Array` 页。

### 无类型写入：`SetValue` / `CopyRange` 的类型不符会抛

那一套算法里绝大多数是**泛型**的（`Fill<T>(T[] array, T value)` 等），值的类型在调用点绑到元素
类型、由编译器把关。只有两个入口**不带类型**，因而校验发生在运行期：

| 入口 | 签名 |
|---|---|
| `SetValue` | `void SetValue(Object value, int index)` —— 形参就是 `Object` |
| `CopyRange` | `void CopyRange(Array source, int, Array destination, int, int)` —— 两侧元素类型可不同 |

**口径是严格的：值的种类必须与元素类型同种，否则抛，且目标数组保持原样。** 抛哪个异常与 C# 相同：

| 情形 | 异常 |
|---|---|
| 某个**值 / 元素**存不进元素类型（`SetValue` 的值；`CopyRange` 里 `object[]` 混着一个不是目标类型的元素，含 null 进值 struct 数组） | `InvalidCastException` |
| `CopyRange` 两侧的**数组类型**本身就不兼容（`int[]` ↔ `Point[]`、`Point[]` → `Vector[]`、`Point[]` → `string[]`） | `ArrayTypeMismatchException` |
| `CopyRange` 的区间越出任一数组 | `ArgumentException` |

不做隐式拓宽 —— `double[]` **不收整数**（`d.SetValue(42, 0)` 抛；要存就传 `42.0`）。

```z42
int[] a = new int[1];
a[0] = 9;
object v = null;
a.SetValue(v, 0);          // ❌ InvalidCastException；a[0] 仍是 9

object n = 42;
a.SetValue(n, 0);          // ✅ 42（整数进 int[] 是正常路径）

string[] s = new string[1];
object none = null;
s.SetValue(none, 0);       // ✅ 引用元素写 null 完全合法

int[] dst = new int[1];
string[] src = new string[1];
Array.CopyRange(src, 0, dst, 0, 1);   // ❌ InvalidCastException；dst 不被改动
```

> 不静默写入 0：`0` 是程序**完全无法与合法写入区分**的答案，所以类型不符一律抛。
>
> **严格而不拓宽是刻意选择**：判据无歧义，且严格版随时可以放宽、反过来不行。

### 值 struct 数组

`Point[]`（`Point` 是 struct）的元素按值内联存放。这些原生入口对它的行为与 C# 一致：

| 入口 | 行为 |
|---|---|
| `Array.Copy` / `CopyRange` | 同类型数组之间按值拷贝（引用类型的字段浅拷贝），同一数组内区间重叠按 memmove 处理 |
| `CopyRange(Point[] → object[] / 接口数组)` | 逐元素装箱 |
| `CopyRange(object[] → Point[])` | 逐元素拆箱；有一个元素不是 `Point` 的装箱（含 null）就抛 `InvalidCastException`，目标数组不动 |
| `GetValue(i)` | 返回元素的**装箱副本**，改它不影响数组 |
| `SetValue(value, i)` | 只收 `Point` 的装箱，按值写进元素 |
| `Array.CreateInstance(typeof(Point), n)` | 一个真正的 `Point[]`，元素为默认值 |
| `Clone()` | 按值的浅拷贝 |

```z42
Point[] src = new Point[] { new Point(1, 2), new Point(3, 4) };
Point[] dst = new Point[2];
Array.Copy<Point>(src, dst, 2);
dst[0].X = 99;             // src[0].X 仍是 1

object[] boxes = new object[2];
Array.CopyRange(src, 0, boxes, 0, 2);   // 两个装箱的 Point
Point p = (Point)boxes[1];              // (3, 4)
```

**还不支持**：`Fill`、`Clear`、`Reverse`、`Resize`、`Sort`、`IndexOf` / `Contains` / `LastIndexOf`、
`Find` 系列、`BinarySearch` 这些**泛型算法**在值 struct 数组上暂不可用（读写元素出错或得到错误结果）——
需要时先用循环按下标逐个处理，或放进 `List<Point>`。只读元素的 `ForEach`、`Exists`、`TrueForAll`、
`FindIndex` / `FindLastIndex`，以及把元素投影成非 struct 类型的 `ConvertAll` 可用。

## 相关

- [集合字面量 `{}`](collection-literals.md) —— List / Dictionary 侧，含两侧统一的脱糖表
- [属性与索引器](properties-indexers.md) —— 用户类型的 `this[...]` 与数组下标的分工
- [迭代](iteration.md) —— `foreach` 遍历数组
