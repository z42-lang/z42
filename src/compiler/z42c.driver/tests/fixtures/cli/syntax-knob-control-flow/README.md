# `[syntax]` 特性门

接线：`z42c.driver/src/Main.z42` 建 feats（未知名在这里报错退出）→ `CompileInput.Features` → `IncrementalDriver` 的**两处**
`new Parser` → `Parser._requireFeature` 发 E0301；特性集折进 `depsId`。

四条判据缺一不可：

- **阳性对照**：不写 `[syntax]` 必须编得过 —— 若连这步都红，后面的红说明不了任何事；
- **负**：`control_flow = false` ⇒ 夹具里的 `foreach` 与 `while` **各**报一条 E0301（不只验退出码：「夹具语法本身出错」也是非零）。
  两个靶子证明门不是只挡 `foreach`；
- **未知特性名** ⇒ 报错退出、不静默忽略（同 `Opt.ByName` 口径），且错因是 `unknown syntax feature`；
- **缓存身份**：只改 toml 不碰源码。用「显式写上 `control_flow = true`」—— 与不写语义等价，所以产物必须逐字节相同；
  但它进 `depsId`，所以 probe 必须不命中（`cached: 0/`，该行在 stderr）。否则「全量生效、增量被忽略」，比从不生效更难查。
  不能改用关掉别的特性来比字节：关掉夹具用到的特性只会编不过。

夹具**必须自带会被 `control_flow` 挡住的语法**，否则翻开关什么都不变、门零判别力。
