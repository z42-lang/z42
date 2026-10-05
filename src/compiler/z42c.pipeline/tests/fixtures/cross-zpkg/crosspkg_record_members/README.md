# crosspkg_record_members — 跨包 `[Record]` 类的成员可用性

**这道门盯的是：导入的 record 必须仍然是 record。**

`target` 导出 `[Record] public class Pt(int X, int Y)` 与一个普通 `class Plain`（对照）；
`main` 对 `Pt` 用 `with` 表达式与位置解构模式 `o is Pt(a, b)`。

## 为什么这是真门

`ConstructTyper._bindWith` 与 `PatternBinder._bindPositional` 以 `!Z42ClassType.IsRecord` 为拒绝条件
（`E0402: \`with\` requires a record type` / `positional pattern requires a record type`）。
本地类的 `IsRecord` 由 `StubCollector._putClassStub` 写入；导入类须经 `CLASS_FLAG_RECORD`（zbc TYPE Flags bit3）
由 `TsigReconcile` 读回、`ExportedClassZ` 承载，`ImportedSymbolLoader` 才能还原。该链路断开时导入 record 会被误拒。

## 为什么 `Plain` 也在 fixture 里

阴性对照。没有它，「fixture 本身接线坏了」与「record 被误拒」两种失败长得一模一样。
