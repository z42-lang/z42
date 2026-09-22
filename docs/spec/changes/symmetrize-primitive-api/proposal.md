# Proposal: primitive API 面对称化（对齐 C#）

## Why

`z42.core` 的 12 个基元包装类型，**同一族 API 一半在一半不在**，而且缺口没有规律可循——
不是「设计上认为这些类型不需要」，纯粹是历次改动按需补、没人回头对过账：

| 缺口 | 现状 |
|---|---|
| `TryParse` | 只有 `Int32` / `Int64` / `Double` 有；`SByte` / `Int16` / `Byte` / `UInt16` / `UInt32` / `UInt64` 全缺 |
| `Single.Parse` | **根本不存在**——想解析 float 只能绕 `Convert.ToSingle`，而它自己内联了 `(float)Double.Parse(s)` |
| `Single` 的 IEEE-754 判定 | `Double` 有 `IsNaN`/`IsInfinity`/`IsPositiveInfinity`/`IsNegativeInfinity`/`IsFinite` 全套 + `NaN`/`±Infinity` 常量；`Single` 一个都没有（源码注释写着「deferred」） |
| `Single.CompareTo` 的 NaN 全序 | 注释说「与 Double.CompareTo 同语义」，**实际没有**那段 NaN 分支 ⇒ 含 NaN 的 `List<float>` 排序行为与 `List<double>` 不一致 |
| `Boolean` / `Char` 的 `Parse`/`TryParse` | 都没有。解析逻辑只存在于 `Convert.ToBoolean` / `Convert.ToChar` 里——与其它基元「`X.Parse` 为主、`Convert.ToX` 委托它」的形状**正好相反** |
| `Convert` 整数族 | 有 `ToByte`/`ToInt16`，缺 `ToSByte`/`ToUInt16`/`ToUInt32`/`ToUInt64` |

这些缺口的共同代价是**使用者无法预期**：知道 `Int32.TryParse` 存在，不代表敢写 `Byte.TryParse`；
知道 `Double.IsNaN` 存在，不代表 `Single.IsNaN` 也在。补齐之后这一族才有「知道一个就知道全部」的性质。

## What Changes

1. **六个窄整型补 `TryParse`**（`SByte`/`Int16`/`Byte`/`UInt16`/`UInt32`/`UInt64`），与既有三个同款：
   成功返回值、格式错或越界返回 `null`（可空返回替代 C# 的 `out` 参数）。
2. **`Single` 补齐到与 `Double` 对等**：`Parse` / `TryParse`；`NaN` / `PositiveInfinity` /
   `NegativeInfinity` 常量（IEEE-754 单精度位模式经 `BitConverter.SingleFromBits`）；
   `IsNaN` / `IsInfinity` / `IsPositiveInfinity` / `IsNegativeInfinity` / `IsFinite`；
   **`CompareTo` 补上 NaN 全序**（此前缺，与 `Double` 分叉）。
3. **`Boolean` / `Char` 补 `Parse` / `TryParse`**，语义对齐 C#（Boolean：去空白 + 大小写不敏感的
   `true`/`false`，其余抛 `FormatException`；Char：恰好一个字符）。`Char` 同时补 `MinValue`/`MaxValue`。
4. **`Convert` 整数族补齐四个**，并把 `ToSingle` / `ToBoolean` / `ToChar` 改为**委托**到对应基元的
   `Parse`——判定只留一处，与 `ToByte`/`ToInt16` 早已如此的形状一致。

## 不做（本 PR）

- **`UInt64.ToString` 对 > i64::MAX 的值渲染成负数**——原计划在本 PR 修（补一个
  `__uint64_to_string` builtin）。**实测这条路走不通**，见下。

### 🔴 实施中查清的真因（原计划因此撤回）

`ToString` 的坑不在 builtin，在**派发**：`vcall()` 不携带接收者的编译期类型名，prim 接收者的类由
`primitive_class_name(obj_val)` **从运行期 `Value` 反推**，而 `Value::I64(_) => Std.Int32`
（`interp/exec_vcall.rs:80`）。`ulong` 与 `long`/`int` 在运行期是同一个 `Value::I64` ⇒
**`UInt64` 上声明的每个实例方法在运行期都到不了**，一律落到 `Std.Int32` 的同名方法。
（`exec_vcall.rs:77-79` 的注释写着「narrow int / long values are tagged with class FQN at
compile-time in VCall instructions」——与 `vcall()` 的实际签名不符，该注释已过期。）

至今无人发现，只因这些类型的 `Equals`/`GetHashCode`/`CompareTo` 实现与 `Int32` 版**恰好等价**；
`ToString` 是第一个真正需要分道的成员。修它要动派发机制（给 VCall 加接收者类型字段 = 又一次
格式 bump，或对已知窄类型的 prim 接收者在编译期改发静态 `Call`），不属 API 对称化的射程。

本 PR 的处置：把真因写进 `UInt64.z42`，并留两条**棘轮测试**钉住当前（错误的）行为——
真因修好那天它们会变红，逼人回来改成正确值，而不是让坑继续静默存在。

## Scope（允许改动的文件）

| 路径 | 变更类型 | 说明 |
|---|---|---|
| `src/libraries/z42.core/src/Primitives/{SByte,Int16,Byte,UInt16,UInt32,UInt64}.z42` | MODIFY | 各加 `TryParse` |
| `src/libraries/z42.core/src/Primitives/Single.z42` | MODIFY | Parse/TryParse + 特殊值常量 + 分类 + NaN 全序 CompareTo |
| `src/libraries/z42.core/src/Primitives/{Boolean,Char}.z42` | MODIFY | Parse/TryParse（Char 另加 Min/MaxValue） |
| `src/libraries/z42.core/src/Convert.z42` | MODIFY | 补四个 ToXxx + 三处改委托 |
| `src/libraries/z42.core/src/Primitives/UInt64.z42` | MODIFY | 仅注释：写清 ToString 的真因 |
| `src/libraries/z42.core/tests/symmetrize_primitive_api.z42` | ADD | 新 API 的测试（每条带对照组） |

## 验证

- `xtask test` 全绿 + `cargo test` 全绿
- 新测试每条都配**对照组**（成功路径 + 失败路径 / 普通值 + 特殊值），防「只要返回值就算过」的退化
- `Single` 与 `Double` 的 NaN 排序逐条对拍（`test_single_matches_double_nan_ordering`）
