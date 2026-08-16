# Tasks: fix-undefined-type-diagnostic

> 状态：🟡 待确认（阶段 6.5）| 创建：2026-08-16 | 类型：lang/semantics（完整流程）

## 进度概览
- [ ] 阶段 1: 诊断码 + Unknown 携名
- [ ] 阶段 2: CheckTypeRef 报 E0443 + 数组递归
- [ ] 阶段 3: 测试与验证

## 阶段 1: 基础
- [ ] 1.1 `DiagnosticCodes.z42`：加 `E0443 UndefinedType`
- [ ] 1.2 `Z42Type.z42`：`Z42UnknownType` 加 `UnresolvedName` 字段 + ctor
- [ ] 1.3 `SymbolTable.z42` `ResolveTypeP` line 217：NamedType 未解析 → 设 `UnresolvedName = nt.Name`

## 阶段 2: 核心实现
- [ ] 2.1 `AccessChecker.z42` `CheckTypeRef`：带名 Unknown → E0443；加 `Z42ArrayType` 元素递归
- [ ] 2.2 单元测试 `z42c.semantics/tests/typecheck/undefined_type/`（正向 6 位置 + 负向 4 场景）

## 阶段 3: 验证
- [ ] 3.1 cargo build (z42vm)
- [ ] 3.2 完整 `xtask test` GREEN（含 compiler 自举 5/5）—— **同时是误报 blast-radius 权威验证**
- [ ] 3.3 REPL 侧 `C c` → `undefined type: C`（Bug 2 表象回归）
- [ ] 3.4 spec scenarios 逐条覆盖
- [ ] 3.5 文档同步：`docs/book/` 类型检查机制页 + z42c.semantics README（若入口/机制变化）
- [ ] 3.6 `docs/agent/rules` 无需改

## 备注
- 若 3.2 GREEN 因某合法类型误报变红 → 停下报告 User（阶段 6.5 中断条件 8：架构性发现），
  按暴露模式调整（预期无：var/泛型形参/嵌套已过滤）。
