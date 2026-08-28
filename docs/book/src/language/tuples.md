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
[Record] struct ValueTuple2<T1, T2>(T1 Item1, T2 Item2);
[Record] struct ValueTuple3<T1, T2, T3>(T1 Item1, T2 Item2, T3 Item3);
// ... 直到 ValueTuple8（元数 2..8；单元素 `(x)` 是括号分组、不是元组）
```

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

- **元数 2..8**；更大元组报错（可后续加 `Rest` 嵌套，如 C#）。
- **链式字段访问 `t.Item1.Item1`**（嵌套元组，泛型 struct 套泛型 struct 槽）会因类型擦除返回错值——
  这是既有泛型-struct-套泛型-struct 的链式访问限制，非元组特有；**先读入局部再访问**（或用模式解构，
  每层读入寄存器）可绕过，元组**模式**因此不受影响。
- **具名元组元素** `(x: int, y: int)`、`Deconstruct` 方法载体、`(T)[]` / `(T)?` 后缀——均后议。

## 相关

- 模式匹配引擎：[模式匹配](pattern-matching.md)
- `[Record]` 与主构造器（元组复用其值语义机制）：[`[Record]` attribute](record-attribute.md)
