# analyzer 诊断活过缓存命中

`[analyzers]` 的诊断与 `[lints]` 的决策不进任何缓存 —— 它们每次构建都在 PackageCompile 里重新跑（cached CU 的 AST 也在）。
但整包全命中时 driver 若在 `no changes; preserved` 处早退、不进 PackageCompile，就有两个静默：

- 什么都不改再构建一次，analyzer 的警告从终端上消失；
- 只改 `[lints]`（升成 error / 开 warnings-as-errors）：源码没动 ⇒ 全命中 ⇒ 构建仍 exit 0，本该判红的诊断被吞掉。

接线点：`z42c.driver/src/Main.z42` 的 `canPreserve`。typecheck 警告活过缓存（进 meta 回放）由 `test compiler incremental` 守；
这里守 analyzer 这一层（不进 meta，只能靠「不早退」）。消费方用 kind=lib：exe 的 preserved 路径多一段侧车 / 依赖装配，与本判据无关。
