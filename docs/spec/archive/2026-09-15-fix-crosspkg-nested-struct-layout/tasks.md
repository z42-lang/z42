# Tasks: fix-crosspkg-nested-struct-layout

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-15

**变更说明：** 导入 struct 的字段类型拼写改与本地同口径（`SurfaceTypeName` 已解析类型），消费方重算的嵌套 struct 布局与生产方一致。
**原因：** `ImportedSymbolLoader._fillClass` 把导出元数据的 FQ 串（`Demo.X.Point3`）照搬进 `OwnFieldSpellings`，`StructLayout._kindOf` 按裸名键查不到 ⇒ 导入 struct 里的嵌套 struct 字段被当 8B 引用叶子 ⇒ 布局错位、读写静默错值。只有嵌套 struct 恰好 8B 时偏移碰巧重合（`struct_cross_pkg` 的 `Point{int,int}`），所以一直没照出来。
**文档影响：** `docs/book/src/runtime/struct-value-semantics.md`「跨包 struct 值语义」机制节。

## 发现经过

修 `fix-generic-struct-chain-access` 时新判据 `_isInlineChainLink` 读 `Layouts.FieldIsStruct`，`struct_cross_pkg` 在 `line.a.x` 崩（`struct ref leaf at byte offset 0 not in type layout`）。探针：导入 `Line` 的 `a`/`b` 拼写为 `Demo.StructTarget.Point`、`isStruct=false`、`off=0/8`。旧链式代码只累加偏移、不看 kind，靠 8B 巧合读对。

实测（修复前编译器，`Point3{int,int,int}` 12B 变体）：期望 `4|6|9|10|11`，实得 `3|5|9|9|10`——**main 上已存在**的静默错值。

## 任务

- [x] 1.1 `ImportedSymbolLoader._fillClass`：`AddOwnField` 拼写改 `SurfaceTypeName(fsym.FieldType)`，Unknown/Error 回落 `fd.TypeName`（对称 `MemberCollector`）
- [x] 1.2 cross-zpkg fixture `struct_nested_layout_cross_pkg`：12B `Point3` 嵌进导入 `Seg`（读 / 写穿不踩兄弟 / 整字段值副本）+ transitive `ext.Frame`（跨包泛型 struct 链式读写三行随 fix-generic-struct-chain-access 那个提交补入）；interp 过；阴性对照（撤 1.1）该 fixture 与 `struct_cross_pkg` 双红
- [x] 1.3 文档同步：struct-value-semantics.md「跨包 struct 值语义」补「字段类型拼写同口径」
- [x] 2.1 `xtask test` 完整 GREEN（与 fix-generic-struct-chain-access 同 PR；base main 6d57a694f，全 stage ✅ 5m44s）
