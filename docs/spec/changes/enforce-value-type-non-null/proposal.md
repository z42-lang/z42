# Proposal: 值类型永不可空

## Why

**「默认值是 null」不是一条策略，是存储初始化根本不看声明类型。**

```rust
let slots = vec![Value::Null; td.fields.len()];   // corelib/assemblyloadcontext.rs:37
```

全仓 6 处这样的站点（见 Scope）。由此产生一整族**运行期内部错误**，而且是反复打补丁、
从未根治的那种：

| 现象 | 出处 |
|---|---|
| `static int N;` 一读就崩 `__box_prim: expected integer value, got Null` | `vm_context/symres.rs`（已打补丁，仅静态字段） |
| 泛型数组未写槽位读出 Null | `interp/exec_array.rs`（已打补丁，注释自称 "Deliberately narrow: only PRIMITIVE value params"） |
| 实例字段全部从 Null 起步 | `corelib/assemblyloadcontext.rs:37` —— **未修** |
| `expected string, got Null` / `ArrayGet: expected array, got Null` 等 | GC / lazy loader 多处 |

补丁都是在**读取侧**兜底，根在**写入侧**（初始化不按声明类型）。

### 编译期同样形同虚设

- `int x = null;` **编译通过**（值类型收 null）
- `int? m = null; int y = m;` 可空赋给不可空，**无诊断**
- `y + 1`（y 实为 Null）→ 运行期 `type mismatch in arithmetic: Null vs I64(1)`
- `Int32.TryParse` 返回 `int?`，而 `?` 在 `SymbolTable.z42:590` 被擦除 ⇒
  `int x = Int32.TryParse(s)` 编译通过，**往 int 槽里塞了一个 Null**

`?` 对值类型没有任何约束力——它是个看起来存在、实际为零的特性。

### 为什么现在做

不需要流分析、不需要标注迁移。实测全仓值类型 `?` 标注只有 **4 个生产 API**
（`Int32` / `Int64` / `Double` / `Guid` 的 `TryParse`）+ 3 个测试文件。

## What Changes

### 值类型永不可空（编译期）

| 写法 | 之后 |
|---|---|
| `int x = null;` | 编译错误 |
| `intExpr == null` / `!= null` | 编译错误（不是静默恒假） |
| `int?` / `T?` 其中 T 是值类型 | 编译错误 |
| `NullableType` 擦除（`SymbolTable.z42:590`） | 值类型分支改为报错；引用类型分支保持擦除 |

### ~~存储按声明类型零初始化（运行期）~~ —— 实测已经是对的

起草时按 grep 判断「6 处 `vec![Value::Null; …]`，实例字段那处未修」。**实测推翻**
（`src/tests/types/value_field_zero/`）：值类型的**实例字段与静态字段都已经读出零值**，
算术也正常（`h.N + 1` == 1）。

那 6 处里 4 处是 `[Native]` 类的 helper（`alloc_native` 等，注释自述「no data slots written;
the class exposes everything via `[Native]` methods」）⇒ 那里的 Null 槽无害；
另两处是 JIT 帧槽与 struct 的引用叶子，Null 本就是引用类型的零值。

⇒ **本变更不改运行期存储初始化**。新增 `value_field_zero` 用例把这个行为钉住，防回归。

⭐ 教训：起草阶段的「grep 出 N 处站点」不等于「这 N 处都需要改」——**先写一个探针实跑**，
比读站点列表快也准。我这条判断是在一棵落后的主树上读代码得出的。

### 拆箱两段检查 —— 拆为 follow-up

实测确认缺口存在：`object o = null; int x = (int)o;` **不抛**，Null 静默落进 int 槽。
根因是基元没有真装箱（`object o = 42` 就是 `Value::I64`）⇒ `(int)o` 是 no-op。

修它要动 cast 路径 + JIT 同步 + 两种异常的分流，是独立一块 ⇒ 拆成 follow-up change
`split-unbox-null-and-type-check`。本变更只交付编译期那半。

原设计如下（保留，供 follow-up 直接用）：

#### 原计划：拆箱两段检查

`object` → 值类型时：**先查 null**（`NullReferenceException`），**再查类型**（`InvalidCastException`）。
两条不同的错、两条不同的消息——错因完全不同，合成一条会让调试变难。

> ⚠️ 这与 #717 的方向相反。#717 把 `__box_prim` 遇 Null 改成静默返回 null，
> 当时的判断是"让一个已存在的错误状态不再抛内部错误"，实际效果是**把警报关掉**。
> 本变更恢复"响"，但换成带位置的用户级异常而非内部错误。

### stdlib TryParse 迁移

值类型不可空 ⇒ `int? TryParse` 无法表达"没有"。按规约改为 `ref` 出参：

```z42
public static bool TryParse(string s, ref int v) {
    try   { v = Int32.Parse(s); return true; }
    catch (Exception e) { v = 0; return false; }
}
```

| API | 现状 | 之后 |
|---|---|---|
| `Int32.TryParse` | `int?` | `bool TryParse(string, ref int)` |
| `Int64.TryParse` | `long?` | `bool TryParse(string, ref long)` |
| `Double.TryParse` | `double?` | `bool TryParse(string, ref double)` |
| `Guid.TryParse` | `Guid?`（`Guid` 是 struct） | `bool TryParse(string, ref Guid)` |
| `IPAddress.TryParse` | `IPAddress?` | **不动**（引用类型，本就正确） |
| `ProcessHandle.TryWait` | `ProcessResult?` | **不动**（引用类型） |

**规约**（写入 reference 文档）：
- 「可能没有」的**引用类型**结果 → `V? Find(...)`，单返回值，不要 bool
- 「可能没有」的**值类型**结果 → `bool TryX(..., ref T v)`，失败写零值
- 这样 **bool 与可空值的组合永远不会出现** ⇒ 不需要 C# 的 `[NotNullWhen(true)]`

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | `:590` NullableType 值类型分支 → 报错 |
| `src/compiler/z42c.semantics/src/DiagnosticCodes.z42` | 新错误码（design §错误码） |
| `src/compiler/z42c.semantics/src/AssignTyper.z42` / `ExprTyper.z42` | null 赋给值类型 → 报错 |
| `src/compiler/z42c.semantics/src/OperatorEmitter.z42` / `ExprTyper.z42` | 值类型 `== null` / `!= null` → 报错 |
| `src/compiler/z42c.semantics/src/TypeNameResolver.z42` | 值类型不再拼 `?` |
| `src/runtime/src/corelib/assemblyloadcontext.rs` | `:37` 实例字段按 tag 零初始化 |
| `src/runtime/src/corelib/diagnostics.rs` | `:38` 同上 |
| `src/runtime/src/corelib/reflection/type_object.rs` | `:281` `:359` 同上 |
| `src/runtime/src/interp/struct_arena.rs` | `:84` struct 的 ref 槽（值槽走 `bytes` 已正确） |
| `src/runtime/src/jit/frame.rs` | `:138` JIT 帧槽（**必须与解释器同步，否则行为分叉**） |
| `src/runtime/src/interp/exec_*.rs` / `jit/helpers/*.rs` | 拆箱两段检查 |
| `src/libraries/z42.core/src/Primitives/{Int32,Int64,Double}.z42` | TryParse 改 `ref` 出参 |
| `src/libraries/z42.core/src/Guid.z42` | `:70` 同上 |
| `src/libraries/z42.core/src/Version.z42` | `:83` 唯一调用点改写 |
| `src/tests/types/nullable_value_types.z42` | 整个文件前提消失 → 改写为阴性用例 |
| `src/tests/types/box_null_nullable.z42` | 同上 |
| `src/libraries/z42.core/tests/scalar_tryparse_classify.z42` | 改 `ref` 写法 |
| `docs/reference/src/language/types.md` 等 | 可空性规则改写 + TryParse 规约 |
| `docs/roadmap.md` | 修正"可空标注"与其它十项塞一行整行标 ✅ 的谎报 |

## Out of Scope

- **引用类型的 `?` 标记 / 流分析 / 反向推导 / `Expect` / 砍 `??` 与 `?.`**
  —— 独立 change `define-null-check-marks`（需流分析设施）
- **definite assignment pass** —— 独立 change
- **泛型 struct 数组的 Null 槽** —— `exec_array.rs` 的补丁注释自述只覆盖 PRIMITIVE 类型参数，
  struct 类型参数仍走老路。这是本变更**已知未堵的洞**，单列 follow-up（强推 struct backing
  会打坏泛型容器，见该处注释）

## Dependencies

**依赖 `simplify-ref-parameters` 先落地** —— TryParse 迁移用 `ref` 出参。
若该变更未过，本变更的 stdlib 迁移部分需改用元组 `(bool, int)`（人体工学较差，见该 change 的讨论）。

## Open Questions

### ✅ Q1 已关闭（2026-09-22）—— 风险不存在

全仓（stdlib 25 包 + 编译器自身 + 全部测试 + scripts）去重后命中 **7 处，全部是 `int?` TryParse 一族**：

| 位置 | 形态 |
|---|---|
| `z42.core/src/Version.z42:84` | `int? v = Int32.TryParse(part); if (v == null)` |
| `z42.core/tests/scalar_tryparse_classify.z42` 6 处 | `Int32.TryParse("abc") == null` 等 |

**零个「值类型字段当 null 哨兵」** ⇒ 零初始化不会静默改掉任何现存语义。
命中的 7 处正是本变更要迁移的 TryParse 路径，随迁移一并消失。

⚠️ **过程中一个假信号值得记**：第一轮只跑 `xtask build stdlib` 得到「0 命中」，那是**缓存跑出来的**
（z42.core 命中 cache，新诊断根本没跑到它）。跑 `xtask test all`（重编更多目标）才露出真实命中。
⇒ **摸底类的「0 命中」必须在无缓存或全量路径上确认**，否则等于没摸。

原始问题记录如下：

**Q1：有没有现存代码用 `f == null` 检测「值类型字段没被设过」？**

零初始化会把这类代码**静默改掉**（从 Null 变 0，判断恒假，不报错）。

grep **查不出来**——按名字匹配全是同名引用字段的误命中。必须在编译器里加一条临时诊断
（「`==`/`!=` 的一侧是值类型」）跑全仓。**这是 tasks 阶段 1 的第一件事，也是本变更唯一的未知数。**
若命中数为 0 → 直接推进；若有命中 → 逐个人工判读后再决定。
