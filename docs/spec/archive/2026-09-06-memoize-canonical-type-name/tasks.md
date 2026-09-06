# Tasks: 类型规范名（Canon）改为每实例记忆 + 内建码投影

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

## 进度概览
- [x] 阶段 1: `PrimModel` 内建码 + 投影表
- [x] 阶段 2: `Z42Type` 每实例记忆
- [x] 阶段 3: 调用点改写 + 删转发器
- [x] 阶段 4: 验证与量化

## 阶段 1: `PrimModel` 内建码 + 投影表
- [x] 1.1 `Code(canon)`：纯字符算术定位（长度 2/3 零字符串比较；4/6 首字符预筛 + 一次完整比较）
- [x] 1.2 四张投影表 `_fq` / `_wrap` / `_kw` / `_tag`，按码序对齐
- [x] 1.3 八个投影（`IsBuiltin` / `IsScalarValue` / `IsInteger` / `IsNumeric` / `Fq` / `Wrapper` /
      `SurfaceName` / `Keyword` / `IrTag`）改表驱动；`IsNumeric`、`SurfaceName` 从两次 Canon 降到一次
- [x] 1.4 `KeywordOfCode` / `IrTagOfCode`：已知码时跳过 Canon+Code

## 阶段 2: `Z42Type` 每实例记忆
- [x] 2.1 基类 `CanonName()` / `PrimCode()` 虚方法 + 四谓词 `IsBuiltinType` / `IsScalarType` /
      `IsNumericType` / `IsIntegerType`
- [x] 2.2 `Z42ClassType` 覆写成惰性字段 `_canon`（哨兵 `""`）/ `_code`（哨兵 `-2`）
- [x] 2.3 `Z42ClassType.Builtin` 灌种：`IsStruct` 与记忆共用一次扫描
- [x] 2.4 `Z42ClassType.IsAssignableTo` 从最多 6 次 Canon 降到 2 次（走记忆则 0 次）

## 阶段 3: 调用点改写 + 删转发器
- [x] 3.1 44 处 `PrimModel.X(t.Name())` / `Z42Type.Canon(t.Name())` 机械改写（15 个文件）
- [x] 3.2 `FunctionEmitter` / `EmitContext` / `OverloadResolver` / `ExprEmitter` 四处
      「`IsBuiltin` 后紧跟 `Keyword` / `IrTag`」合并成一次 `PrimCode` + `*OfCode`
- [x] 3.3 删 `Z42Type.Canon` 转发器（自身占 0.53%），调用点直呼 `PrimModel.Canon`
- [x] 3.4 内联掉 `PrimModel.CodeOf` 转发层

## 阶段 4: 验证与量化
- [x] 4.1 **验收门 1（产物逐字节）**：改前/改后 driver 各编一次 `src/libraries/z42.net`
      （本变更改的是 `z42c.semantics` 自身，按规矩换参照包）
      → `.zpkg` `963fc176…`、`.zsym` `328d1785…` **两侧完全相同**（rebase 前后各验一次）
- [x] 4.2 **验收门 2**：`xtask test compiler` → `3/3 packages gen1==gen2`
- [x] 4.3 **验收门 3**：`xtask test` → `✅ GREEN — all stages passed`
- [x] 4.4 **验收门 4**：同机交错 A/B（同一份输入源码，两个 driver 交替各 4 次）
      → 指令 66.210 G → **63.308 G（−4.38%）**、墙钟 5.485 → **5.218 s（−4.87%）**、
      峰值 RSS 993.3 → **901.4 MB（−9.25%）**（rebase 到 `813a8c13` 后重测）

## 留给后续（本次刻意不做）
- `Z42ClassType.Builtin` onstack 1.03%：每次合成都新建一个类型对象（含三个 `StrMap`）。
  intern 掉能省，但会改对象身份且 `Fields` / `Methods` / `Namespace` 可写 —— 独立评估。
- `StructLayout` 那批以**裸字符串**为键的 `Canon` 调用（`Z42Type.Canon` 调用方的 47%）：
  没有类型对象可挂记忆，要动得先看调用点手里有没有 `Z42Type`。
