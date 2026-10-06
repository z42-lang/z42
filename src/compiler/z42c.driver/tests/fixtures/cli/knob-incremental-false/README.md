# `[build] incremental = false` 真的生效

`[build] incremental` 解析进 `BuildConfig.Incremental`，接线点是 `z42c.driver/src/Main.z42` 的 `incrOff`。
`test compiler incremental` 的对账（增量 dist == 全量 dist）测不到这个开关：一个从不生效的开关恰恰不会让两者不等。

判据取**行为**：同一份源连编，默认第二次必须命中缓存（stdout 打印 `preserved`，阳性对照 —— 否则要么增量整体坏了，
要么判据依赖的输出措辞变了）；`incremental = false` 之后不得出现 `preserved`。改 toml 之后要再建**两次**：
改 toml 那一次本身就可能因清单变化而不命中，只有第二次才能说明旋钮在起作用。
