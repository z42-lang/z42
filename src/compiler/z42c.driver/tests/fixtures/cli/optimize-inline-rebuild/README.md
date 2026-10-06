# `[optimize]` 生效且进包级缓存键

`[optimize]` 解析进 `ProjectManifest.Optimize*`，接线点是 `z42c.driver/src/Main.z42` 的 `effOptSet`（`Opt.FromToml` +
`Opt.Resolve`）；若只在全量构建生效而增量不生效，查那里把优化集并进 `depsId` 的那一行。

判据取**产物字节**，不靠日志措辞：

- `inline = true` 全量构建，记下 dist；
- 只改 toml 成 `inline = false`、源码一字不动，**增量**构建 ⇒ 字节必须变。这一步同时验旋钮生效与缓存身份：
  优化集若不在包级 cache key 里，probe 全命中、产物不变 —— 「全量生效、增量被忽略」比从不生效更难查；
- 改回 `true` ⇒ 必须回到原字节（可复现）。

夹具**必须自带可内联的调用**：`Add` 是小函数、在循环里被调用。换成没有可内联调用的工程，`inline` 翻哪边字节都一样，
本用例就成了恒红（或判据写反时恒绿）的摆设 —— 判别力来自夹具，不来自判据。
