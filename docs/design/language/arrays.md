# z42 Arrays（数组）规范

> **Status**: L1 ✅ ｜ 一维数组 + Std.Array 基类（spec/archive/2026-05-07-add-array-base-class）

## 设计参考

| 来源 | 借鉴点 |
|------|--------|
| **C#** | 语法：`T[]`、`new T[n]`、`new T[]{ ... }`、`.Length`、引用语义 |
| **Rust** | `Vec<T>` 的越界 panic 模型；不允许未初始化访问 |

---

## Phase 1 范围

只支持**一维动态数组**（对应 C# `T[]`），多维数组和 jagged array 留到 Phase 2。

---

## 语法

```csharp
// 字面量初始化
int[] arr = new int[] { 1, 2, 3 };

// 指定长度（零值初始化：int→0, bool→false, string→"", object→null）
int[] arr2 = new int[n];

// 元素读写
int x = arr[0];
arr[1] = 42;

// 长度
int len = arr.Length;
```

### 集合字面量（add-collection-literals，2026-08-07）

方括号 `[]` 是数组的专属字面量语法（借鉴 JSON/JS `[]`＝数组、C# 集合初始化器、Rust 重复填充）。
与 `{}`（List / Dictionary，见 [collection-literals.md](collection-literals.md)）互补：**`[]` 一律数组**。

```z42
// 1. 元素字面量（target-typed 元素类型；var → 元素公共类型）
int[] xs = [1, 2, 3];               // 等价 new int[]{ 1, 2, 3 }
var   ys = [10, 20, 30];            // → int[]
int[] e  = [];                      // 空数组（元素类型来自目标；var 无目标 → 报错）

// 2. 重复填充 [value; count]（Rust 风；value 只求值一次；count 可运行期）
int[] zeros  = [0; 100];
int[] sevens = [7; n];

// 3. spread 展开 [..a, x, ..b]（拼接数组片段 + 散元素）
int[] cat = [..xs, 99, ..ys];       // 本轮 spread 源仅数组
```

**脱糖（纯前端，零新 IR / 零格式 bump）**：

| 形态 | 脱糖为 |
|------|--------|
| `[e0, e1, ..]`（无 spread） | `BoundArrayLit`（同 `new T[]{...}`） |
| `[v; n]`                    | `$c=n; $a=new T[$c]; for $i<$c { $a[$i]=v }`（`v` hoist 一次） |
| `[..a, x, ..b]`             | 各 spread 源 hoist → `new T[Σ长度]` → 逐段拷贝循环 |

- 元素公共类型：无目标时取首元素类型（`[1, 2L]` 这类混合数值以目标类型加宽；否则以首元素为准）。
- 空 `[]`：必须有目标元素类型（`var x = []` 报错）。
- 实现原理（合成 AST + `BoundSeqExpr` 序列表达式）见
  [`docs/spec/changes/add-collection-literals/design.md`](../../spec/changes/add-collection-literals/design.md)。

### 类型表示

| z42 类型 | 描述 |
|----------|------|
| `int[]`    | int 数组 |
| `string[]` | string 数组 |
| `T[]`      | 任意类型的一维数组 |

数组是**引用类型**（堆分配），赋值传递引用（与 C# 一致）。

---

## 语义

- 索引越界：运行时 panic（VM 抛出错误，对应 C# `IndexOutOfRangeException`）
- `.Length` 返回 `int`
- 元素类型检查：编译期由 TypeChecker 验证（`arr[i] = v` 中 v 必须可赋值给元素类型）

---

## IR 映射

新增 4 条指令：

| IR 指令 | 操作 |
|---------|------|
| `array_new { dst, size }` | 分配 size 个元素的零值数组 |
| `array_new_lit { dst, elems: [reg...] }` | 字面量数组 |
| `array_get { dst, arr, idx }` | 读元素，越界 panic |
| `array_set { arr, idx, val }` | 写元素，越界 panic |
| `array_len { dst, arr }` | 返回长度 (i32) |

### 示例：`new int[] { 1, 2, 3 }`

```
%r0 = const.i32 1
%r1 = const.i32 2
%r2 = const.i32 3
%arr = array_new_lit [%r0, %r1, %r2]
```

### 示例：`arr[i] += 1`（结合复合赋值）

```
%v   = array_get %arr, %i
%one = const.i32 1
%v2  = add %v, %one
       array_set %arr, %i, %v2
```

---

## VM 扩展（Rust）

`Value` 枚举新增：
```rust
Array(Rc<RefCell<Vec<Value>>>)
```

- 用 `Rc<RefCell<...>>` 实现引用语义和可变性
- 越界时 `bail!("array index {} out of bounds (len={})", idx, len)`

---

## TypeChecker 扩展

- `T[]` 对应 `Z42ArrayType { elem: Z42Type }`（已在 `Z42Type.cs` 定义）
- `new T[n]`：检查 `n` 为 `int`
- `arr[i]`：检查 `arr` 为数组类型，`i` 为 `int`，结果类型为元素类型
- `.Length`：仅允许在数组类型上访问，返回 `int`

---

## 运行时基类 `Std.Array`（add-array-base-class，2026-05-07）

所有 `T[]` 在运行时是 `Std.Array : Object` 的实例。stdlib `src/libraries/z42.core/src/Array.z42` 定义 sealed class，绑定 `Length` 字段（IR `array_len` 快路径）+ `Clone()` / `GetType()` / `Equals()` / `GetHashCode()` / `ToString()`（全 Native 绑定到现有 `__obj_*` / `__array_clone` builtin）。

类型系统：`T[]` is-a `Std.Array` is-a `Std.Object` —— 真子类型链（`Z42Type.IsAssignableTo` 显式分支，不再依赖 `target == Object` catch-all 对数组的兜底）。

VM dispatch：`Value::Array` 不携带 TypeDesc 引用，VCall 通过 `primitive_class_name` 路由到 `Std.Array.<method>` 直查 func_index（与 `Std.Int32` / `Std.String` 同款）；`is_instance` / `as_cast` 硬编码识别 `Array` / `Object` / `Std.Array` / `Std.Object` 子类型。

后续 follow-up：静态算法（Sort/IndexOf/...）、IEnumerable 接入、协变（`Dog[]` → `Animal[]`）、元素类型反射元数据（`arr.GetType().__name == "int[]"`）—— 各自独立 spec。

## 不在此规范范围内

- 多维数组 `T[,]` / jagged `T[][]`（Phase 2）
- `Array.Sort`、LINQ、`IEnumerable`（Phase 2）
- 协变数组（Phase 2）
