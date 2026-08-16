# Tasks: fix-undefined-type-diagnostic

> 状态：✅ 完成（GREEN 全绿，待归档 + PR）| 创建：2026-08-16 | 类型：lang/semantics（完整流程）

## 进度概览
- [x] 阶段 1: 诊断码 + Unknown 携名
- [x] 阶段 2: CheckTypeRef 报 E0443 + 数组递归
- [x] 阶段 3: 测试与验证

## 阶段 1: 基础
- [x] 1.1 `DiagnosticCodes.z42`：加 `E0443 UndefinedType`
- [x] 1.2 `Z42Type.z42`：`Z42UnknownType` 加 `UnresolvedName` 字段 + ctor
- [x] 1.3 `SymbolTable.z42` `ResolveTypeP` fallthrough：NamedType 未解析 → 设 `UnresolvedName = nt.Name`

## 阶段 2: 核心实现
- [x] 2.1 `AccessChecker.z42` `CheckTypeRef`：带名 Unknown → E0443；加 `Z42ArrayType` 元素递归
- [x] 2.2 单元测试 `z42c.semantics/tests/typecheck/undefined_type/`（正向 6 位置 + 负向 5 场景）

## 阶段 2.5: design 未预见的两处修正（实施中发现）
- [x] 2.3 `ExprTyper._bindNew`：删 `new C()` 的 `E0401 unknown type in new` 特例——否则与 E0443 双报；
  统一由 CheckTypeRef 报 E0443（Scope 外文件，已记 spec.md MODIFIED + design 注）
- [x] 2.4 `SymbolTable.ResolveTypeP`：`var` 字段（`public static var x=…`）经 collector choke point
  时 type-name="var" 落 fallthrough → 误报 `undefined type: var`；加 `if (n=="var") return 匿名 Unknown`
  推断哨兵（镜像 void）。首轮 GREEN 挂 `generic_field_carry`+`var_field_cross_pkg` 暴露此漏

## 阶段 3: 验证
- [x] 3.1 cargo build (z42vm)（GREEN 自建）
- [x] 3.2 完整 `xtask test` GREEN（含 compiler 自举 5/5 字节不动点 + stdlib 247 + cross-zpkg）——
  误报 blast-radius 权威验证：首轮暴露 var-字段假阳性（已修）、二轮全绿
- [ ] 3.3 REPL 侧 `C c` → `undefined type: C`（可选终验；需 fresh toolchain，GREEN 不编 scripting）
- [x] 3.4 spec scenarios 逐条覆盖（含 MODIFIED `new`→E0443）
- [x] 3.5 文档同步：`access-control.md`（CheckTypeRef E0443 机制小节）+ `error-codes.md`（E04xx 例）
- [x] 3.6 `docs/agent/rules` 无需改

## 备注
- design D3「var 已过滤」只覆盖**局部** var（StmtBinder）；**字段** var 走收集期 choke point 未过滤
  → 实施中修（阶段 2.4）。教训：choke point 复用时，各调用相位的前置过滤未必对称。
