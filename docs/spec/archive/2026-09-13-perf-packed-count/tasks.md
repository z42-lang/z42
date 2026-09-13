# Tasks: perf-packed-count

> 状态：🟢 已完成 | 创建：2026-09-13 | 完成：2026-09-13
> 类型：`perf`（compiler）—— 走[最小化模式](../../../../.claude/rules/workflow.md)。

**变更说明：** `SymbolTable` 的三张「打包串」表（`ClassNsAll` / `ClassPkgAll` / `FuncOriginAll`，值是
`|` 拼接的 `StrBox`）在热路径上不再 `Split`；项数改为写时记在 `StrBox.Count`、读时 O(1)。

**原因：** 跨包重复诊断（E0601 `CrossPkgDuplicateMsg` + E0606 `ShadowedImportMsg`）在**每个类型引用**上
成对被调，裸名歧义判据（E0456 `IsBareNameAmbiguous`）在每个发射的类型引用上被调 —— 三者都只需要
「有几项」，却各 `Split` 出一个数组 + N 个子串。2026-09-13 profile：`Std.String.Split` 自占 **3.34%**，
其中 57% 由 `ClassPkgsOf` 驱动、41% 由 `ClassNsOf`。

**文档影响：** 无外部行为变更（判据一字未改，只改「怎么取」）。

- [x] 1.1 `StrBox` 加 `Count`（ctor 置 1）；`SymbolTable.NoteNsInto` —— 三张表**唯一**的追加点 —— 维护它
- [x] 1.2 `CrossPkgDuplicateMsg` / `ShadowedImportMsg` / `IsBareNameAmbiguous` 改读 `Count`；
      `ShadowedImportMsg` 在单包时连 `pkgs[0]` 都不必取（`box.Value` 即是）
- [x] 1.3 写入路径审计：`SymbolCollector._mergeImports` 是**逐项**喂回 `NoteNsInto` 的（先 Split 再一条条
      `NoteClassNs`），故 `Count` 在所有路径上都对；没有「把已拼好的多项串直接塞进 StrBox」的写法
- [x] 1.4 A/B（`/usr/bin/time -l`，各 3 跑）+ 复采 profile 确认 Split 真的掉下去了
- [x] 1.5 `xtask test` 全 13 stage 绿

## 备注

**中途一个失败的版本，值得记下**：先用 `packed.IndexOf("|") < 0` 判单项 —— **净收益几乎为零**。
profile 显示 `Split` 3.34%→0.07% 的同时 **`IndexOf` 2.78%→5.39%**（47% 来自新判据）：单项串里没有
分隔符，`IndexOf` 必须**扫完整串**才能确定，O(n) 且无提前退出。省下的只有分配那部分（RSS −1.2%）。
⇒ 这类判据要真便宜，只能**写时记好**，不能读时现算。

**`SymbolTable.z42` 已到行数天花板**：本次改完是 **886/886**，零余量。文件在改前就是 868。
门禁（`xtask test lines`）确实在本次先判了红（901 行），故把新增注释压到了限内 —— 但**下一次动它的人
必然要先拆**。拆分按 code-organization.md 应是独立的 refactor commit，不该混进性能改动，故**未做**，
作为独立事项留给 User 裁决（候选：把「名字来源 / 歧义」那一簇 —— `NoteNsInto` / `ClassPkgsOf` /
`ClassNsOf` / `FuncOriginsOf` / 三个 Msg 方法 / `IsBareNameAmbiguous` —— 搬进 `SymbolOrigins`
静态类，**调用方直接调、不留转发壳**，因为「一层纯转发 ≈ 0.5% 全负载」）。
