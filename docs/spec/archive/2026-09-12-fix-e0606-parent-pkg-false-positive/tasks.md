# Tasks: E0606 对测试目标的父包误报

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`fix`（最小化模式）

**变更说明：** 测试/bench 目标引用**父包**的任何类型都被判 E0606（本地遮蔽导入），
而那正是测试该做的事。main 因此在**冷构建**下全红。

**原因（两个各自正确的变更撞在一起）：**
- `z42b-owns-test-targets`（#578）为了让测试看见父包的 `internal`，把父包按**同包**待遇加载
  —— 其类型 `IsImported = false`。
- `report-crosspkg-duplicate-type` + #577 把「本地遮蔽导入同 FQN」从 W0606 **升级为 E0606**。

于是父包的同一个类型既被记成「本包声明」（IsImported=false）又被记成「导入包声明」
（`ClassPkgAll` 里有它的包名）⇒ 判定成立 ⇒ 每一处父包类型引用都误报。
**实际上一共就一份类型**，没有第二份被顶掉、也没有谁指不到 —— 判据的前提不成立。

**修法（改在信息源头）：** `ImportedSymbolLoader` 记录「FQN → 来源包」时**跳过父包**。
这张表的语义是「有几个**别的**包声明了它」，父包对本次编译不是「别的包」。
类 / 接口 / enum 三处记录点同款处理。E0601（两包同 FQN）同理不再误报。

> 曾试过在 `SymbolTable` 上挂 `ParentPkg` 字段、在 `ShadowedImportMsg` 里豁免 —— **无效**：
> 检查看到的符号表没走 `_mergeImports`（探针显示该字段恒空）。源头修法既更短也更准。

**为什么本地 GREEN 之前没拦住：** 夹具的构建产物是**热**的（缓存命中不重编），
错误只在冷构建出现。CI 恒冷，所以是 CI 先红。

**文档影响：** 无（诊断语义未变，只是不再对父包误报）。

- [x] 1.1 `ImportedSymbolLoader.Load`：三处 `ClassPkgAll` 记录点跳过 `parentPkg`
- [x] 1.2 GREEN：`xtask test` 全 13 stage 绿；`xtask test targets` 的 dev-target 夹具
      （`tests/dirunit/` 断言「目录单元看得见父包 internal」）由红转绿
