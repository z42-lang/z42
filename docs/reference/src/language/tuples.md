# 元组（值元组 `(a, b)`）

元组是轻量的多值分组：`(int, string)` 是类型，`(7, "hi")` 是字面量，`(x, y)` 是模式。z42 的元组
是**值类型、零堆分配**（对齐 C# `System.ValueTuple`、Rust/Swift/Go 的值元组），它补齐了模式匹配引擎的
最后一块结构化载体。

```z42
using Std;

(int, string) mkPair() { return (7, "hi"); }        // 元组返回类型 + 字面量

void demo() {
    (int, string) p = (7, "hi");                    // 元组类型 var-decl + 字面量
    int a = p.Item1;                                // 字段访问 .Item1 / .Item2 ...
    (x, y) = mkPair();                              // 解构声明
    switch (p) {
        case (0, s):   /* ... */ break;             // 元组模式 + 位置常量子模式
        case (n, s):   /* ... */ break;             // 位置绑定
    }
    if (p is (m, n)) { /* ... */ }                  // is 元组模式
}
```

## 设计：元组 = 合成的泛型 `struct` 值类型（路线 A）

z42 **不引入原生元组 opcode / 类型 tag**。元组在编译器前端**脱糖**为 `Std` 命名空间里一族合成的
泛型 `[Record] struct`：

```z42
[Record] public struct ValueTuple2<T1, T2>(T1 Item1, T2 Item2);
[Record] public struct ValueTuple3<T1, T2, T3>(T1 Item1, T2 Item2, T3 Item3);
// ... 直到 ValueTuple8（元数 2..8；单元素 `(x)` 是括号分组、不是元组）
```

> 🔴 **`public` 是必需的，不是修饰性的**（`fix-binder-emitter-gaps-batch2`，欠债表 bug B2）。
> 脱糖目标由**用户工程的代码**引用，而 z42 类的默认可见性是 internal（= 包内）⇒ 这七个类型
> 一旦漏掉 `public`，**任何 z42.core 之外的包写 `(1, 2)` 都撞 E0404**
> `cannot access internal class ValueTuple2 from another package`——修前正是如此，且**每一种
> 元组写法都撞**（`var t = (1,2)` / `(int,int) t = (1,2)` / 跨包元组形参与返回一律）。
> 编译器脱糖引用的其余类型（`Attribute` / `Dictionary` / `List` / `Type` / `IDisposable` /
> `InvalidOperationException` / `Bencher`）早已 public，只漏了这一组。
> 门在 `src/tests/cross-zpkg/tuple_cross_pkg/`——它走 `z42c build`（诊断可见、非零退出即失败）；
> `src/tests/tuples/tuple_basic.z42` 走 `--emit-zbc`（吞诊断），修前是**假绿**。

它们定义在 `src/libraries/z42.core/src/ValueTuple.z42`（隐式 prelude，任何程序自动可见）。这样元组
**复用了泛型 `struct` record 的全部既有机制**——blob 值布局、值语义 `Equals`/`GetHashCode`/`ToString`
合成、构造、字段访问、以及模式匹配的字节偏移读——**零新 IR、零新发射代码、零格式 bump**。

### 为什么零格式 bump

zbc / zpkg 里类型引用一律 intern 进字符串池（非封闭 tag enum），故「又多一个字符串
`ValueTupleN`」不需要任何二进制格式变更；与泛型当年落地同款。原生 tuple opcode（`tuple.new` /
`tuple.get`）才会 bump，而 z42 的 blob struct 本就无对象头、原生 opcode 边际收益极小，不值。

### 运行时表示：类型擦除的均匀槽

泛型 `struct` record 在 z42 VM 里是**类型擦除**的：布局按**泛型定义名**（如 `ValueTuple2`）注册一份，
每字段占**均匀 8 字节槽**，与实例化的元素类型无关。`(int, string)` 与 `(long, long)` 共享同一
`struct_alloc ValueTuple2 [16B]` 布局，字段偏移 `@0` / `@8`；元素值（含基元）以擦除槽承载，读回时按
消费点的静态类型驱动解释。这是既有泛型 struct 机制，元组直接沿用。

## 三层脱糖流程

| 表面语法 | 脱糖目标 | 落点 |
|---------|---------|------|
| 元组类型 `(T1, ..., Tn)` | `NamedType("ValueTupleN", [T1..Tn])` | `TypeParser._parseParenType`（解析括号类型列表后按尾随 `->` 分流：有 `->` = 函数类型，否则 ≥2 元素 = 元组、1 元素 = 括号分组） |
| 元组字面量 `(e0, ..., en)` | `new ValueTupleN(e0, ..., en)`（泛型实参由构造实参推断） | `ExprParser` 括号分组分支（首元素后遇 `,` 收集为 `TupleExpr`）→ `ConstructTyper._bindTuple` → 复用 `_bindNew` |
| 元组模式 `(p0, ..., pn)` | `BoundPositionalPattern`（绑定于 `ValueTupleN` 实例化类型，字段 `Item1..ItemN`） | `PatternParser`（裸 `(` 起始）→ `PatternBinder._bindTuple` → 复用既有模式发射（struct blob 字节偏移读、`needTest=false` 不发 IsInstance） |

**用短名 `ValueTupleN`（非 FQ `Std.ValueTupleN`）**：z42.core 是恒加载 prelude，其类型以裸短名注册进
符号表，短名恒可解析；FQ 点分名反而不经 using 解析路径。

### 什么算「元组类型」

判据是**整名精确匹配** `ValueTupleN`（N ∈ 2..8，短名或带 `Std.` 前缀都认），对应 `z42.core` 里
真实声明的那 7 个 `[Record] struct`。**自己的类型名里带 ValueTuple 不会让它变成元组**：

```z42
struct MyValueTupleBox { public int A; public int B; }
void f(MyValueTupleBox b) {
    switch (b) { case (x, y): ... }     // ✗ E0402：它不是元组，与任何普通类型一视同仁
}
```

> 2026-09-24 之前判据是「名字里**含** ValueTuple」（子串匹配），于是上面这段**编译零诊断**、
> 还真去按 blob 字节偏移解构；而逐字段完全同形、只是名字正常的 `PlainBox` 报 E0402 ——
> 能力按名字子串分叉。位置解构本身对任意形状都成立，挡住它的一直只是这条名字检查。

### 元素类型从哪来

元组字面量脱糖成 `new ValueTupleN<t0..tn>(e0..en)` 时，**类型实参是在脱糖处写出来的**
（先绑各元素取其类型，经 `_typeToTypeExpr` 反解成类型表达式），不是留裸名交给构造实参去推。

这一步是必需的：`new` **不做类级型参推断**，写成裸的 `new ValueTupleN(...)` 时结果类型就是
泛型定义本身，元素静态类型停在擦除的 `T1..Tn`。做法与集合字面量（`{1,2,3}` → `List<int>`）同款。

> 2026-09-22 之前正是漏了这一步，于是**凡是用 `var` 接元组字面量，元素类型全丢**：
>
> ```z42
> var t = (1, 2);
> t.Item1 + t.Item2          // ✗ E0402: operator + requires numeric operand, got T1
>
> var n = ((1, 2), 3);
> ((a, b), c) = n;           // ✗ E0402: tuple pattern requires a tuple-typed subject, got T1
> n.Item1.Item2;             // ✗ 运行期 FieldGet: expected object, got StructRef
> ```
>
> 写显式类型（`(int, int) t = (1, 2)`）一直是好的 —— 那条路的实例化类型由变量声明的目标类型
> 提供，压根没经过脱糖里的推断。平坦解构 `(a, b) = t` 也一直「能用」，但那只是因为绑定子模式
> 不做类型检查，`a` / `b` 的静态类型其实也是 `T1` / `T2`。现已全部修正。

### 语句位歧义消解（`(` 开头）

`(` 在语句 / 顶层声明位有多种含义，靠**配平括号后的随后 token**分流：

| 形态 | 判据 | 结果 |
|------|------|------|
| 元组类型 var-decl `(int, string) p = ...` | 顶层含 `,` + `)` 后跟**标识符** | `_isVarDeclStart` → 变量声明 |
| 元组解构声明 `(a, b) = e` | `)` 后跟 `=` | `_isDeconstructDeclStart` → 解构声明 |
| 函数类型 var-decl `(T) -> R f = ...` | `)` 后跟 `->` | `_isVarDeclStart`（既有） |
| 元组表达式语句 `(a, b);` | `)` 后跟 `;` | 落表达式语句 |
| 顶层自由函数 `(int, string) f() {...}` | 顶层声明位 `(` 起始 | `Parser` 顶层分派 → `_parseTopLevelFunc` |

## 应用位点

元组模式统一接入模式引擎三位点：`switch` 臂、`is` 表达式、解构声明 `(x, y) = e`。子模式可递归（嵌套
元组 `((a, b), c)`、位置常量 `(0, s)`、通配 `(_, y)`、绑定 `(n, s)`）。因模式解构**逐层把元素读入新
寄存器**再递归，嵌套元组安全。

## 限制（v1）

- **元数 2..8**；更大元组报 `E0402: tuples support between 2 and 8 elements, got N`
  （可后续加 `Rest` 嵌套，如 C#）。**类型位与字面量位同码同文案**。

  > 2026-09-24 之前上界**只在字面量侧**校验：9 元组**类型**照样合成出内部名 `ValueTuple9`，
  > 于是用户拿到的是 `E0443: undefined type: ValueTuple9` —— 一个自己从没写过的名字。
  > 同一件事在字面量侧一直有准确诊断。本节这行「更大元组报错」当时只兑现了一半。
- 嵌套元组的**链式字段访问** `t.Item1.Item2` 可直接读写（2026-09-15 fix-generic-struct-chain-access
  修复了显式类型那条路；`var` 那条路到 2026-09-22 才补齐，见下方「元素类型从哪来」）。
  但嵌套在元组里的 struct 值目前**不是独立副本**——struct 的值复制语义见
  [所有权与内存模型](memory-model.md)。
- **具名元组元素** `(x: int, y: int)`、`Deconstruct` 方法载体、`(T)[]` / `(T)?` 后缀——均后议。

## 相关

- 模式匹配引擎：[模式匹配](pattern-matching.md)
- `[Record]` 与主构造器（元组复用其值语义机制）：[`[Record]` attribute](record-attribute.md)
