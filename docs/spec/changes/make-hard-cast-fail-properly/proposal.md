# Proposal: 硬转换 `(T)x` 失败时正确报错

## Why

**`as` 和 `(T)x` 在 Bound 层是同一个节点**（`TypeOpTyper._bindAsExpr` 与 cast 都产出 `BoundCast`），
于是发同一条 `AsCastInstr`，而该指令实现的是 **`as` 语义**（`exec_object_isa.rs::as_cast`：
`Value::Null => true` 放行一切、失配返 `Value::Null`）。

⇒ **硬转换在 z42 从来不抛。** 实测三种形态（探针已跑，2026-09-22）：

| 写法 | 现状 | 应当 |
|---|---|---|
| `object o = new Other(); Box b = (Box)o;` | **不抛，把错类型的对象原样返回** | `InvalidCastException` |
| `object n = null; int x = (int)n;` | `InvalidCastException: cannot convert Null to type tag 0x04` —— **内部错误串，`catch (Exception)` 抓不到**，消息是 Rust Debug 格式 | `NullReferenceException`（带源位置，可 catch） |
| `object s = "hello"; int y = (int)s;` | 同上（`cannot convert Str("hello") to type tag 0x04`） | `InvalidCastException`（可 catch） |

第一行最坏：**类型系统被静默绕过**，错类型的对象一路往下流，到某个不相关的地方才以别的形态崩。

而且 stdlib **根本没有 `InvalidCastException` 类**（`src/libraries/z42.core/src/Exceptions/` 下 18 个
异常类里没有它）；运行期那两条只是 `semantics.rs` 里的 `bail!` 字符串 —— 内部错误，
不走异常机制，因此不可 catch。

### 与刚合入的可空线的关系

`enforce-value-type-non-null`（#741）在**源码层**建立了「值类型永不含 null」，但拆箱是它的
唯一逃逸路径：`(int)someNullObject` 把 Null 送进 int 槽。该 change 的 tasks 把这条拆为
follow-up，起初以为只是「拆箱两段检查」；实测发现根因是整个硬转换语义缺失，故重新定范围。

## What Changes

### 硬转换降解成「先查后转」，**零格式 bump**

`BoundCast` 加 `IsHardCast` 标志（`as` 置假、`(T)x` 置真）。发射期对硬转换降解成：

```
if (!(x is T)) { throw <适当的异常>; }
<原来的 AsCast>
```

全部用**现有指令**（`IsInst` + 分支 + `ObjNew` + `Throw`），不新增 opcode、不动 wire 格式。

异常的选择按 C# 对齐：

| 情形 | 异常 |
|---|---|
| `x` 是 null，目标是**值类型** | `NullReferenceException` |
| `x` 是 null，目标是**引用类型** | **不抛** —— null 可以转成任何引用类型（C# 同）|
| `x` 非 null 且 `!(x is T)` | `InvalidCastException` |

### 新增 `InvalidCastException`

`src/libraries/z42.core/src/Exceptions/InvalidCastException.z42`，与既有 18 个异常类同形。

### 运行期两条 `bail!` 改为真异常

`semantics.rs` 的数值转换路径在源不是数值时 `bail!("InvalidCastException: …")` ——
内部错误、不可 catch、消息是 Rust Debug 格式。改为构造真异常（复用
`crate::exception::make_*` 路径），消息用用户可读的类型名。

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/libraries/z42.core/src/Exceptions/InvalidCastException.z42` | NEW |
| `src/compiler/z42c.semantics/src/BoundExprOp.z42` | `BoundCast.IsHardCast` |
| `src/compiler/z42c.semantics/src/TypeOpTyper.z42` | `_bindAsExpr` 置假；cast 绑定点置真 |
| `src/compiler/z42c.semantics/src/TypeOpEmitter.z42` | `_emitCast` 对硬转换降解「先查后转」 |
| `src/runtime/src/semantics.rs` | 两条 `bail!` → 真异常 |
| `src/runtime/src/jit/**` | 若 JIT 另有 cast 快路则同步 |
| `src/tests/types/**` | 三种形态各一个正/阴用例（interp + jit） |
| `docs/reference/src/language/conversions.md` | 硬转换的失败语义 |
| `docs/reference/src/appendix/error-codes.md` | 若新增诊断码 |

## Out of Scope

- `as` 的语义不动（失配返 null 是对的）
- 数值窄化转换（`(int)3.7`）不动 —— 那条走 `ConvertInstr`，与本变更无关
- 格式 bump（刻意避开）

## Open Questions

### ✅ Q1 已解（按构造，不需实测）

原问题：若 `is` 对跨包接口的判定与 `as` 不一致，「先查后转」会**误抛**。

**不会。** 运行期两者调的是**同一个** `isa_td(ctx, &module.type_registry, rc.type_desc(), class_name)`
（`exec_object_isa.rs` 里 `is_instance` 与 `as_cast` 的 `Value::Object` 臂逐字相同），
未装箱裸基元也都回落同一个 `prim_isa`。语义层两个节点（`BoundIsExpr.TypeName` /
`BoundCast.TypeName`）也都用 **AST 原始类型名**——那正是修跨包接口失配的手法。

⇒ 现在 `(T)x` 能过的，`x is T` 必为真 ⇒ 检查不会误抛。

**唯一的不一致点是 null**：`as_cast` 有 `Value::Null => true`（放行一切），而 `is` 对 null 返假。
设计里已单独处理：null + 引用类型目标 → **不抛**（C# 同）；null + 值类型目标 → `NullReferenceException`。

无其它未决项。
