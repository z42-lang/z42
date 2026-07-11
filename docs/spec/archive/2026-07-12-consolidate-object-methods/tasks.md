# Tasks: Object-4 导出方法单源（refactor）

> 状态：🟢 已完成 | 完成：2026-07-12

**变更说明：** ExportedTypeExtractor（AST 导出）与 TsigReconcile（IR 重建）各自 _m0/_m1 构造
Object 四方法（ToString/Equals/GetHashCode/GetType），字面同构 → 抽到 z42c.ir `ObjectMethods.Four()`。
**原因：** compiler-review §4.3 的可行切片；直接修 P2/P3 的 Rebuild 引入的第三处重复（drift 风险）。
**文档影响：** compiler_review.md §4.3 状态。

- [x] 1. z42c.ir/src/ObjectMethods.z42：`Four() -> ExportedMethodZ[4]`（含 _m0/_m1）
- [x] 2. ExportedTypeExtractor：4 行 → ObjectMethods.Four()；删本地 _m0/_m1
- [x] 3. TsigReconcile：同上
- [x] 4. build + fixpoint + GREEN

## 备注
- SymbolCollector 的 Object-4 是 MethodSymbol（符号表），非 ExportedMethodZ → 不共享（不同类型），保持独立。
- 字节不变：Four() 产同一批 ExportedMethodZ → fixpoint 守。
