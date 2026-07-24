# Tasks: 推断 var 字段类型（infer-var-field-types）

> 状态：🟡 实现完成，本地端到端验证通过，全量 GREEN 待 CI（冷自举链本地不可验）| 创建：2026-07-24
> 类型：`fix`（编译器根因，产出端）| 子系统：`compiler`（独立分支 `claude/infer-var-field-types`）

## 进度概览
- [x] 根因定位 + 双通道确认
- [x] fixup pass（VarFieldInfer）+ IrDump 接线
- [x] TYPE 段通道（ClassDescBuilder）
- [x] e2e fixture + 端到端验证
- [ ] CI 全量 GREEN（self-host 不动点 + stdlib + compiler 单测）

## 实现
- [x] 1.1 `VarFieldInfer.z42`：绑定 static var 字段初始化器 → 推断 → 回写 `fs.FieldType`（fixpoint ≤8 轮）
- [x] 1.2 `IrDump.BuildPackageCus`：SymbolCollect 后、Export 前插入 `VarFieldInfer.Run(cus, count, symbols)`
- [x] 1.3 `ClassDescBuilder.z42:141`：TYPE 段 var 字段用 `fs.FieldType.Name()`（非 var 保留源拼写）

## 验证（本地，用 warm-z42c 回路）
- [x] 2.1 跨包 var 字段算术编译通过（原 E0402）
- [x] 2.2 端到端运行：跨包 var 字段 carry + 算术 → 105；字符串拼接 → z42!
- [x] 2.3 **A/B 回归**：原版 vs 改后 semantics 编 z42.collections **字节完全一致**（无 var 字段 → 零漂移）
- [x] 2.4 e2e fixture `cross-zpkg/var_field_cross_pkg` 建立并跑通（105 / z42!）
- [ ] 2.5 CI：self-host 7/7 gen1==gen2 不动点（z42c 0 处 public static var → 预期零漂移）+ `xtask test stdlib`/`compiler`/`cross-zpkg` 全绿

## 备注
- **双通道**：TSIG（VarFieldInfer 回写 fs.FieldType）+ TYPE 段（ClassDescBuilder）——只修 TSIG 无效，
  跨包解析读 TYPE 段（drop-tsig-expt 后）。实施期实证发现。
- **无回归依据**：只改 var 字段路径；z42c 源 0 处 public static var 字段；A/B 证 z42.collections 字节不变。
- **下游**：`add-z42-repl` 的 D7 carry-forward 状态模型依赖本 fix（跨轮 carry 前轮 var 字段需真实类型）。
- 实例 var 字段跨包同缺陷 → follow-up `infer-var-field-types-instance`（Out of Scope）。
