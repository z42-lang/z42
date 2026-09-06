# Proposal: 类型规范名（Canon）改为每实例记忆 + 内建码投影

## Why

标准负载（`z42c build z42c.semantics --release`，28 k 行、66.2 G 指令）的采样 profile 里，
`PrimModel.Canon` 子树占 **4.69%**，是 `#490` 之后剩下的最大单簇。它的形状是「频次问题」，
不是「单次算法问题」——`Canon` 本体在 `#448 B` 已经优化过（`CharAt` 直判 + 按 (长度,首字符) 分桶），
非内建名只剩 4~7 次 native 分发。真正的浪费在两处：

**① 同一个类型对象被反复问同一个名字。** 全仓 44 处调用形如 `PrimModel.X(t.Name())`，
`t` 是 `Z42Type`、`_name` 构造后不变，却在绑定 / 转换 / 发射三个阶段各重扫一遍字符串。
极端例子是 `Z42ClassType.IsAssignableTo`：一次调用里 `IsBuiltin` / `Canon` / `IsScalarValue` ×2 /
`Keyword` ×2 合计触发 **最多 6 次 Canon**，而只涉及两个名字。

**② 每个投影在 Canon 之后再跑一条最多 14 次字符串 `==` 的链。**
`IsBuiltin` / `IsScalarValue` / `IsInteger` / `Fq` / `Wrapper` / `Keyword` / `IrTag` 都是
`string c = Canon(n); if (c == "i8") … if (c == "object") …`，用户类名要走完整条链才落空。
`IsNumeric` 更是调 `IsInteger`（一次 Canon）之后自己再 Canon 一次；`SurfaceName` 同理
（`IsBuiltin` + `Keyword` = 两次 Canon）。

采样归因（占全负载 self%）：`Canon` 子树 4.69%，加上各投影自身的比较链自占约 1.5% ⇒ **约 6.2%**。

## What Changes

- `PrimModel` 引入 **内建码**（0..13，-1 = 非内建）：`Code(canon)` 用纯字符算术定位
  （长度 2/3 桶**一次字符串比较都不做**；长度 4/6 桶按首字符预筛后才落一次完整比较，
  防 `"book"` / `"strict"` 之类用户类名被首字符误判）。四张投影表 `_fq` / `_wrap` / `_kw` / `_tag`
  按码序对齐，所有投影退化成一次数组索引。
- `Z42Type` 增加 `CanonName()` / `PrimCode()` 两个虚方法 + 四个谓词
  （`IsBuiltinType` / `IsScalarType` / `IsNumericType` / `IsIntegerType`）。
  基类现算；`Z42ClassType` 覆写成**惰性字段**（`_canon` / `_code`）。
  `Z42ClassType.Builtin` 构造时顺手灌进记忆（`IsStruct` 与后续查询共用同一次扫描）。
- 44 处 `PrimModel.X(t.Name())` 机械改写成 `t.XType()` / `t.CanonName()`。
- 删掉纯转发器 `Z42Type.Canon`（`return PrimModel.Canon(n);`）——profile 里它**自身**就占 0.53%，
  调用点一律直呼 `PrimModel.Canon`。同理内联掉新加的 `CodeOf` 转发层。

## Non-Goals

- **不做全局 `StrMap` memo**：memo 的 key 查找要 `GetHashCode`（O(n) FNV-1a，profile 里 1.88%），
  比 `Canon` 现在的 4~7 次分发还贵，大概率净负。
- **不 intern `Z42ClassType.Builtin`**：会改对象身份，且 `Fields` / `Methods` / `Namespace` 可写，
  风险不对称。（它 onstack 1.03%，留作独立候选。）
- 不碰 `StructLayout` 那批以**裸字符串**为键的 `Canon` 调用（没有类型对象可挂）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/PrimModel.z42` | MODIFY | 新增 `Code` / `KeywordOfCode` / `IrTagOfCode` + 四张投影表；八个投影改表驱动 |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `CanonName` / `PrimCode` + 四谓词；`Z42ClassType` 惰性记忆；删 `Z42Type.Canon` 转发器 |
| `src/compiler/z42c.semantics/src/*.z42`（15 个） | MODIFY | 44 处调用点机械改写 |
| `src/compiler/z42c.semantics/tests/typecheck/typecheck_tests.z42` | MODIFY | 注释里的 `Z42Type.Canon` 更名 |

## 验收

1. **产物逐字节不变**（本变更改的是 `z42c.semantics` 自身 ⇒ 按记忆里的规矩换 `src/libraries/z42.net`
   当参照）：改前 driver 与改后 driver 各编一次 `z42.net`，`.zpkg` / `.zsym` 的 sha 必须相同。
2. `xtask test compiler` → `3/3 packages gen1==gen2`。
3. `xtask test` → `✅ GREEN — all stages passed`。
4. 量化：同机交错 A/B（同一份输入源码，两个 driver 交替各跑 4 次），报指令数 / 墙钟 / 峰值 RSS。
