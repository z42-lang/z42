# Tasks: primitive API 面对称化（对齐 C#）

> 状态：🟢 GREEN 全绿，待提交 | 创建：2026-09-22 | 完成：2026-09-22
> 变更类型：feat（stdlib API 新增，纯增量、无删改）
> 文档影响：无 book 机制页变更（新增 API 语义与 C# 同款，行为由测试固化）
> **基线**：起初叠在 `drop-short-primitive-aliases`（PR #730）上以避开 12 个 Primitives 文件的
> 硬冲突；#730 已 squash 合并进 main（`97de57a98`），本分支随即 `rebase --onto origin/main`
> 摘成单 commit，对 main 是干净 diff。rebase 后已按 parallel-development §3 重跑完整 GREEN。

## 进度概览
- [x] 阶段 1: 六个窄整型补 `TryParse`
- [x] 阶段 2: `Single` 补齐到与 `Double` 对等
- [x] 阶段 3: `Boolean` / `Char` 补 `Parse` / `TryParse`
- [x] 阶段 4: `Convert` 整数族补齐 + 三处改委托
- [x] 阶段 5: 测试（每条带对照组）
- [x] 阶段 6: 验证 GREEN

## 阶段 1: 窄整型 TryParse
- [x] 1.1 `SByte` / `Int16` / `Byte` / `UInt16` / `UInt32` / `UInt64` 各加 `TryParse`
      （try/catch → `null`，与既有 `Int32`/`Int64`/`Double` 逐条同款）

## 阶段 2: Single 对等化
- [x] 2.1 `Parse`（走 `Double.Parse` 再窄化 —— VM 把 float/double 同存 `Value::F64`）+ `TryParse`
- [x] 2.2 `NaN` / `PositiveInfinity` / `NegativeInfinity` 常量
      （单精度位模式经 `BitConverter.SingleFromBits`；`0xFF800000` 超出 int 正区间会被当 long
      字面量，故 NegativeInfinity 用等值的有符号十进制 `-8388608` 写）
- [x] 2.3 `IsNaN` / `IsInfinity` / `IsPositiveInfinity` / `IsNegativeInfinity` / `IsFinite`
- [x] 2.4 **`CompareTo` 补 NaN 全序** —— 此前注释声称「与 Double.CompareTo 同语义」而实际没有，
      含 NaN 的 `List<float>` 排序会被污染（NaN 的 `<`/`>` 都 false ⇒ 塌成 0 ⇒ 与所有值相等）

## 阶段 3: Boolean / Char
- [x] 3.1 `Boolean.Parse` / `TryParse`（去空白 + 大小写不敏感的 true/false，其余抛）
- [x] 3.2 `Char.Parse` / `TryParse`（恰好一个字符）
- [x] 3.3 `Char.MinValue` / `MaxValue` —— **与 C# 有意不同**：z42 的 `char` 是 32 位 Unicode
      标量值（非 C# 的 16 位 UTF-16 码元），故上界取 `0x10FFFF` 而非 `0xFFFF`

## 阶段 4: Convert
- [x] 4.1 补 `ToSByte` / `ToUInt16` / `ToUInt32` / `ToUInt64`（整数族八个齐）
- [x] 4.2 `ToSingle` / `ToBoolean` / `ToChar` 改为委托到对应基元的 `Parse`
      （判定单一出处；此前 Boolean/Char 的逻辑只存在于 Convert 里，与其它基元形状相反）

## 阶段 6: 验证
- [x] 6.1 `xtask test` 全 stage 绿
- [x] 6.2 `cargo test` 1447 passed / 0 failed（rebase 到 main 最新 `b82282d93` 后重跑，仍全绿）
- [x] 6.3 新测试 19 条，每条配对照组（成功+失败 / 普通值+特殊值）；
      `Single` 与 `Double` 的 NaN 排序逐条对拍

## 🔴 撤回的一项 + 查清的真因

原计划在本 PR 修 **`UInt64.ToString` 对 > i64::MAX 渲染成负数**（补 `__uint64_to_string` builtin）。
**实测走不通，已撤回**（builtin 与改绑都已回滚，不留不可达的死 builtin）。

真因不在 builtin 而在**派发**：`vcall()` 不携带接收者的编译期类型名，prim 接收者的类由
`primitive_class_name(obj_val)` 从运行期 `Value` 反推，而 `Value::I64(_) => Std.Int32`
（`interp/exec_vcall.rs:80`）。`ulong` 与 `long`/`int` 在运行期同为 `Value::I64` ⇒
**`UInt64`（以及 SByte/Int16/Byte/UInt16/UInt32）上声明的每个实例方法在运行期都到不了**，
一律落到 `Std.Int32` 的同名方法。

- 为何至今无人发现：这些类型的 `Equals`/`GetHashCode`/`CompareTo` 实现与 `Int32` 版**恰好等价**。
  `ToString` 是第一个真正需要分道的成员。属 [[silent-feature-masks-other-bugs]] 的同族。
- `exec_vcall.rs:77-79` 的注释写着「narrow int / long values are tagged with class FQN at
  compile-time in VCall instructions」—— 与 `vcall()` 的实际签名不符，**该注释已过期**。
- 修法要动派发机制（VCall 加接收者类型字段 = 又一次格式 bump；或对已知窄类型的 prim 接收者
  在编译期改发静态 `Call`），不属 API 对称化的射程，另案。
- 本 PR 的处置：真因写进 `UInt64.z42`；留两条**棘轮测试**钉住当前（错误的）行为 ——
  真因修好那天它们会变红，逼人回来改成正确值，而不是让坑继续静默存在。
