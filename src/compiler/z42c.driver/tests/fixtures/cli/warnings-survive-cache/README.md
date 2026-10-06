# typecheck 警告活过缓存命中

这类缺陷不动产物一个字节 —— 增量与全量的 zbc 相同，丢的只有终端上的警告行，所以判据看 **stderr**；
`test compiler incremental` 的「产物对账全绿」与「警告一条看不见」可以同时成立。

两个独立的静默器，各管一种缓存形态，所以两格都要测（只修一格，另一格会让「构建两次」这个最常见的用法继续静默）：

- **整包全命中**：driver 在 `no changes; preserved` 处早退、压根不编译 ⇒ 接线点是 `z42c.driver/src/Main.z42`
  preserved 早退前的警告回放；
- **部分命中**：`CompileCuTask.Run` 的 cached 分支把 `DiagMsgs` 置空 ⇒ 接线点是 `z42c.pipeline/src/PackageCompile.z42`
  的 `CachedNsMeta` 回填 + `CacheStore` 的 `diag` 行。

两个文件缺一不可：警告在 `a.z42`，可被单独改动的 `b.z42` 用来制造部分命中（`cached: 1/2`）—— 单文件工程只能试出全命中那一格。
kind = lib 是刻意的：exe 还有 Main / entry 探测，在 preserved 路径上多跑一段，与本判据无关。
analyzer 那一层诊断（不进 meta，只能靠不早退）由 `analyzer-diag-survives-cache` 守。
