# 数组

> 对齐：2026-08-07（change `add-collection-literals`）；`Std.Array` 基类 2026-05-07
> （change `add-array-base-class`）

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

- **索引越界 = VM abort，不是可 catch 的异常**：VM 直接以
  `array index {i} out of bounds (len={n})` 终止（`src/runtime/src/interp/exec_array.rs:196`）。
  没有 `IndexOutOfRangeException`，`try`/`catch` 接不住。
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

**口径是严格的：值的种类必须与元素类型同种，否则抛 `Std.Exception`，且目标元素保持原值。**
不做隐式拓宽 —— `double[]` **不收整数**（`d.SetValue(42, 0)` 抛；要存就传 `42.0`）。

```z42
int[] a = new int[1];
a[0] = 9;
object v = null;
a.SetValue(v, 0);          // ❌ 抛；a[0] 仍是 9

object n = 42;
a.SetValue(n, 0);          // ✅ 42（整数进 int[] 是正常路径）

string[] s = new string[1];
object none = null;
s.SetValue(none, 0);       // ✅ 引用元素写 null 完全合法

int[] dst = new int[1];
string[] src = new string[1];
Array.CopyRange(src, 0, dst, 0, 1);   // ❌ 抛；dst 不被改动
```

> 🔴 **历史**（fix-silent-array-elem-zero，2026-09-27 修正）：这两个入口此前**静默把目标元素
> 存成 0**（`0` / `'\0'` / `false` / `0.0`），不抛、报成功。`int[0]` 原值 9 被 `SetValue(null, 0)`
> 变成 **0**；`double[]` 收一个整数——一个在 C# 里合法的写法——也被静默变成 0。
> 而 `0` 是程序**完全无法与合法写入区分**的答案。
>
> ⚠️ 只在 **release** 如此：debug 构建一直会在那里 panic，所以本地/CI 的 debug 语料看不见它。
>
> **严格而不拓宽是刻意选择**：判据无歧义，且严格版随时可以放宽、反过来不行。

## 相关

- [集合字面量 `{}`](collection-literals.md) —— List / Dictionary 侧，含两侧统一的脱糖表
- [属性与索引器](properties-indexers.md) —— 用户类型的 `this[...]` 与数组下标的分工
- [迭代](iteration.md) —— `foreach` 遍历数组
