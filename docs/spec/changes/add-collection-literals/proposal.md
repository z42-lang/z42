# Proposal: 集合字面量（`[...]` / `{...}` + Rust 重复填充 + spread）

> Status: **IMPL 完成**（2026-08-07；e2e 448/0 interp+jit 全绿）
> 分类：lang（新语法）→ 走规范先行流程
> 子系统：compiler（纯前端）
> 实现原理见 [design.md](design.md)；用户文档见 arrays.md / collection-literals.md

## Why

z42 目前建集合噪音很重，是所有 example 里最啰嗦的高频句式：

```z42
int[] arr = new int[] { 1, 2, 3 };              // 数组：右侧重复类型 + new + []{}
var list = new List<int>(); list.Add(1); ...    // List：只能逐个 Add
var m = new Dictionary<string,int>(); ...        // Dict：同上
```

对照其他语言，构造集合本应是一两个字符的事：Swift/C#12 `[1, 2, 3]`、Python `{"a": 1}`、
Rust `[0; 100]`（重复填充）。本变更把这套简化引入 z42。

**关键点：纯前端 desugar，零运行时 / 零 IR / 零格式 bump。** `[...]` / `{...}` 在语义层
lower 成现有的 `ArrayInitExpr`（`new T[]{}`）/ `new List<T>` + `Add` / `new Dictionary<K,V>` + 赋值，
产出的 zbc 与手写等价代码逐字节相同。因此 **不需要 zbc/zpkg minor bump**，唯一约束是
[两阶段 nightly 纪律](../../../.claude/rules/bootstrap-seed.md)的**语法轴**（support 先行、晚一个
nightly 才能在 z42c/stdlib 源码里 use）。

## 语法总览：`[]` = 数组，`{}` = 花括号族（List / Dict / 对象）

借鉴 **JSON/JS**（`[]` 数组、`{}` 对象）+ **C# 集合初始化器**（`{}` 建集合）：方括号一律数组，
花括号是 List / Dict / 对象初始化器的共用外壳，靠**内容**消歧。

| 字面量 | 归属 | 判据 |
|--------|------|------|
| `[1, 2, 3]` `[0; n]` `[..a, ..b]` | **数组 `T[]`（专属）** | 方括号一律数组 |
| `{1, 2, 3}` | **`List<T>`** | 花括号 + 裸元素 |
| `{"a": 1, "b": 2}` | **`Dictionary<K,V>`** | 花括号 + `key: value` 对 |
| `new Foo { X = 1 }` | 对象初始化器（**Change 2**） | `new Type` 前缀 + `字段 = 值` |
| `[]` / `{}`（空） | 由**目标类型**定 | 无目标类型 → 报错 |

## What Changes

### 1. 数组字面量 `[...]`（专属数组，不再 target-typed 摇摆）

```z42
int[] xs = [1, 2, 3];           // → new int[]{ 1, 2, 3 }
var   zs = [1, 2, 3];           // → int[]（恒为数组，T 取元素公共类型）
```

### 1b. List 字面量 `{...}`（裸元素）

```z42
List<int> ys = { 1, 2, 3 };     // → new List<int>(); ys.Add(1); ys.Add(2); ys.Add(3);
var       ws = { 1, 2, 3 };     // → List<int>（花括号裸元素默认 List）
```

### 2. Rust 重复填充 `[value; count]`

```z42
int[] a = [0; 100];             // 100 个 0
int[] b = [7; n];               // n 可为运行期值
```

- `value` **只求值一次**（对齐 Rust），再填 `count` 份。
- `value` 为默认零值时 desugar 到 `new T[count]`（已零初始化，零开销）；非零值时 desugar 到
  `new T[count]` + fill 循环（前端生成）。
- 引用类型 `[obj; n]`：n 份**同一引用**（z42 数组/对象是引用语义），文档明确标注。

### 3. spread 展开 `[..a, ..b, x]`

```z42
int[] cat = [..xs, 99, ..ys];   // 拼接 xs、字面 99、ys
```

- `..expr` 的 `expr` 须为同元素类型的数组 / List。
- desugar：前端算总长 → `new T[total]` → 逐段 copy（段来自 `.Length` + 索引拷贝）。

### 4. 字典字面量 `{key: value, ...}`（花括号 + 冒号对）

```z42
Dictionary<string,int> m = { "a": 1, "b": 2 };   // → new Dictionary + 逐项赋值
var m2 = { "a": 1 };                              // 花括号 + k:v → Dictionary<K,V> 由元素推
```

**List vs Dict 在 `{}` 内靠内容区分**：元素含 `key: value`（冒号对）→ Dict；裸元素 → List。
同一 `{}` 内混用（部分带冒号部分不带）→ 报错。

### 5. 空字面量 `[]` / `{}`（必须有目标类型）

`[]` 恒为空数组（元素类型由目标定）；`{}` 是 List 还是 Dict 由目标类型定：

```z42
int[]                  e1 = [];   // → new int[0]
List<int>              e2 = {};   // → new List<int>()
Dictionary<string,int> e3 = {};   // → new Dictionary<string,int>()
var bad1 = [];                    // 错误：空数组字面量需显式元素类型
var bad2 = {};                    // 错误：空 {} 无法判定 List / Dict，需显式目标类型
```

## 核心设计决策（DRAFT 待确认）

### D1. `{...}` 的三重身份消歧（**crux**）

`{` 在 z42 有多重身份：语句块、List 字面量、Dict 字面量、`new Foo { }` 对象初始化器（Change 2）。
两级消歧：

1. **位置消歧（块 vs 花括号字面量）**：`{...}` 只在**表达式位置**（RHS、实参、return、集合元素…）
   解析为 List/Dict 字面量；**语句位置的 `{` 永远是块**。花括号字面量作**裸表达式语句**
   （`{ 1, 2 };`）**不允许**——无用途，归块解析。z42 无块表达式，故位置即可判定，无二义。
2. **内容消歧（List vs Dict vs 对象）**：进入花括号字面量后看内容——
   - 首元素形如 `expr : expr`（冒号对）→ **Dict**；
   - 首元素为裸 `expr`（无冒号、无 `=`）→ **List**；
   - `字段 = 值`（含 `new Type` 前缀）→ **对象初始化器**（Change 2，本轮报"暂不支持"）；
   - 空 `{}` → 由目标类型定（List / Dict），无目标类型报错。

### D2. `[...]` 与索引 / 切片的消歧

`[` 作**前缀**（表达式起头）= 数组字面量；作**后缀**（`expr[i]` / `expr[a..b]`）= 索引/切片。
前缀 vs 后缀在 Pratt 解析里天然区分，无冲突。`[...]` **恒为数组**，不再有数组/List 二义。

### D3. target-typed 的传播范围（本轮边界）

目标类型来源本轮**只认三处**：局部变量声明左侧类型、`return` 的函数返回类型、赋值左侧。
**不含**：作为实参按形参类型反推（需与重载决议交互）、泛型实参推断。无目标时按默认：
`[...]`→`T[]`、`{裸元素}`→`List<T>`、`{k:v}`→`Dictionary<K,V>`；空 `[]`/`{}` 无目标 → 报错。

### D4. 元素公共类型推断

`[1, 2, 3]`→`int[]`、`{1, 2, 3}`→`List<int>`；`[1, 2L]`→`long[]`（数值提升）；混不出公共类型 →
报错（本轮不引入 `object[]` 兜底，避免意外装箱）。Dict 的 K/V 各自独立推断公共类型。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | 新增 `ArrayLitExpr`（`[]` 元素 + spread 标记）、`ArrayRepeatExpr`（value+count）、`ListLitExpr`（`{}` 裸元素）、`DictLitExpr`（`{}` k/v 对） |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | 前缀 `[` → 数组/重复/spread；表达式位置 `{` → List/Dict（内容消歧 D1） |
| `src/compiler/z42c.syntax/src/StmtParser.z42` | MODIFY | 语句位置 `{` 仍解析为块（确认消歧不误伤） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | target-typed 定型 + 元素公共类型推断（D3/D4）+ 空字面量校验 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | desugar：→ `ArrayInitExpr` / `new List`+Add / `new Dictionary`+赋值 / 重复填充 / spread 拼接 |
| `docs/design/language/arrays.md` | MODIFY | 集合字面量 + 重复 + spread 语法节 |
| `docs/design/language/language-overview.md` | MODIFY | 声明与初始化简化概览 |
| `examples/collection_literals.z42` | NEW | 示例（数组/List/重复/spread/字典/空） |
| `src/tests/collection-literals/*.z42` | NEW | golden：各形态端到端 |

**只读引用**：`Ast.z42` 现有 `ArrayInitExpr`/`ObjNewExpr` 节点、`ExprEmitter.z42` 现有 array-init
codegen、`.claude/rules/bootstrap-seed.md`（语法轴纪律）。

## Out of Scope

- **对象初始化器 `new Foo{X=1}` / 字段简写 `{x,y}` / 结构更新 `..base`** → Change 2（`..base`
  还依赖未 merge 的 struct 值语义，见 memory `struct-value-semantics-program`）。
- **实参按形参反推 target type**（D3 边界）、泛型实参推断集合字面量。
- **`object[]` 混类型兜底**（D4）。
- **z42c / stdlib 源码使用 `[...]`**：两阶段纪律，晚一个 nightly 的 follow-up。本轮只落"支持"。
- JIT / AOT：纯前端 lowering，VM 路径不变。

## Open Questions

- [ ] 重复填充 `[v; n]` 的 `n` 是否限制编译期常量？倾向**允许运行期**（z42 数组本就动态），
      非零 `v` 走 fill 循环。
- [ ] spread 是否只在数组 `[..a, ..b]` 支持，还是 List `{..a, ..b}` 也支持？倾向本轮
      **只数组**，List spread 留后续。
- [ ] `{}`（List/Dict）目标为 `var` 时默认 `List<T>` / `Dictionary<K,V>` 是否 OK？
      （stdlib 当前唯一实现）
- [ ] `{1, 2, 3}` 记 List 而非 Set：z42 无 Set 字面量需求，确认无歧义。
