# Tasks: split-symbol-origins

> 状态：🟢 已完成 | 创建：2026-09-13 | 完成：2026-09-13
> 类型：`refactor`（compiler）—— 纯代码搬运，无行为变更。

**变更说明：** `SymbolTable` 拆成两个 `partial` 文件：核心留 `SymbolTable.z42`（886 → 636 行），
「名字来源 / 歧义 / 跨包重名」那一簇搬进 `SymbolTable.Origins.z42`（292 行）。

**原因：** `SymbolTable.z42` 在 #627 之后是 **886/886，零余量**；`xtask test lines` 当时确实判红过
（901 行），只是把新增注释压回限内才过。下一个动这个文件的人必然先被门禁挡住。

**文档影响：** 无外部行为变更（方法体逐字未改，判据 / 消息措辞 / 调用关系全不动）。

- [x] 1.1 新建 `SymbolTable.Origins.z42`（`public sealed partial class SymbolTable`），搬入 19 个方法：
      三条打包串表的读写原语（`NoteNsInto` / `NoteClassNs` / `NoteFuncOrigin` / `ClassPkgsOf` /
      `ClassNsOf` / `FuncOriginsOf` / `_pkgBoxOf` / `_nsBoxOf`）、五条诊断消息（E0456 / E0601 / E0606，
      类与自由函数各一套）、`IsBareNameAmbiguous` / `IsScopeVisibleNs` / `_orig*`
- [x] 1.2 `SymbolTable.z42` 的类声明加 `partial`；**字段全部留在原处**（ctor 与 `WithAliases` 在那边）
- [x] 1.3 机械核对「纯搬运」：把新旧两侧的非注释代码行做多重集比较 —— 差异为 0
- [x] 1.4 `xtask test` 全绿；自举字节不动点连跑 3 次均 3/3

## 备注

**为什么用 `partial` 而不是抽成静态辅助类**：后者要么改所有调用点、要么留一层转发壳，而这条仓库
**实测**过「一层纯转发 ≈ 0.5% 全负载」。partial 是零调用点改动、零额外帧的拆法，且是本仓既定做法 ——
`Std.String` 就拆成 `String.z42` + `String.Edit.z42` + `String.Split.z42` 三个 `sealed partial class`。

**⚠️ 这没有解决「类型 200 行硬限」**，也不假装解决了。`code-organization.md` 明写「类型拆多文件时
累计计入……防止『类型超 200 行』被散落到多文件来绕过」—— `SymbolTable` 作为**类型**仍是约 900 行
（拆分前后不变，本来就 4 倍超限）。本次只解决**被机械门禁执行的那条**（文件 886 行）。

真要修类型超限得做**真正的分解**（`SymbolTable` 现在同时管：符号存储 / 类型解析 `ResolveTypeP` /
歧义诊断 / 接口图 / 约束表），而那一簇的难点是**它横跨「共享表」与「per-CU 作用域状态」**
（`IsBareNameAmbiguous` / `AmbiguousBareNameMsg` 要 `ScopeNs` + `ScopeUsings`，而那是 per-view 的，
`WithAliases` 每个文件一份）—— 拆成独立对象要先决定作用域状态归谁。这是**设计问题、需要裁决**，
不该夹在一次 refactor 里顺手定。

**一次值得记的假红**：首轮 `xtask test` 报 `自举不动点 1/3 packages gen1≠gen2`（z42c.semantics，
**同尺寸不同内容** = 顺序差异）。产物收敛后连跑 3 次均 3/3、完整门禁绿。机制：本改动改变了
`SymbolTable` 在 z42c.semantics 里的成员布局，而首轮比较时 gen1 仍由「改动传播前的编译器」所建 ——
要多走一代才收敛（正是两代自举存在的理由）。**冷启动那条以 CI `verify-selfhost` 为准**：它从上一个
nightly 种子起步建 gen1、再由 gen1 建 gen2 并逐字节比对，本地无法忠实复现（naive `rm -rf
artifacts/build/compiler` 会退化成「旧种子一代编当前源」，撞的是 #610/#618 的新符号，与本改动无关）。
