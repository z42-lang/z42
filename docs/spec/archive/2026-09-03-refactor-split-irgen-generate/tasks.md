# Tasks: 拆分 IrGen.Generate（refactor-split-irgen-generate）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（compiler；三面评审 C-10 文本结构阶段 ②）
**变更说明：** `IrGen.Generate` 是 408 行的单体（60 行硬限的 6.8×），把类 / 成员 / impl / 接口 / enum / delegate / 自由函数的
发射全部内联。按职责提出 `IrGenSink`（模块级累加器）、`IrGenMemberEmitter`（方法 / 属性 / 索引器）、`IrGenTypeEmitter`
（类 record + 成员循环 + 合成 ctor / struct Equals / record 合成 + impl）、`IrGenAuxEmitter`（接口 / enum / delegate / 自由函数）；
`Generate` 收敛为 ~70 行编排（非空非注释 <60）。**逻辑逐字搬移、发射顺序不变 → 产物 byte-identical**。
**原因：** code-organization.md 函数 60 行硬限；`IrGen.z42` 642 行 → 307 行（脱离 500 行硬限，棘轮基线可剔除）。
**文档影响：** `src/compiler/z42c.semantics/README.md`（核心文件表）；`scripts/test/line-limit-baseline.txt`（IrGen.z42 剔除，
若本 change 晚于 add-line-count-lint 合并）。

- [x] 1.1 `IrGenSink.z42` / `IrGenMemberEmitter.z42`（146 行）/ `IrGenTypeEmitter.z42`（141 行）/ `IrGenAuxEmitter.z42`（114 行）
- [x] 1.2 `IrGen.Generate` 改编排；`IrGen.z42` 642 → 307 行
- [x] 2. 字节对账：base（main 2a8d3219 工具链）vs 本分支工具链编全部 stdlib + z42c 成员包，逐包 `cmp`
- [x] 3. `xtask test` GREEN（含自举不动点）
- [x] 4. 文档同步 + 归档

## 备注
- 本 change 即 main 上既有 change `split-irgen-class`（2026-07-12，review P1-3）的最后一步 4b；两者随本 PR 一起归档。

## 字节对账（2026-09-03）
base = wt-c8 工具链（main 2a8d3219 + perf-tsig-reconcile-index，后者已证明产物逐字节相同）；new = 本分支工具链；同一源码树、同一 z42vm；
每包 `z42c build <toml> --release` 到独立目录后 `cmp` zpkg：**26/26 相同**（25 个 stdlib 包 + `z42c.semantics` 自身）；
`z42c.pipeline` / `z42c.driver` 两包 base 侧因 base libs 目录缺其依赖成员而未能编译（环境问题，非本 change），由 GREEN 的自举不动点覆盖。
