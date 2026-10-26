# plugin-single-file-in-tree — 单文件插件住消费方树内

单文件条目 `igen/igen.z42` 住在 app 树**内**，且 app 的 `[sources].include = ["**/*.z42"]` 会匹配到
它——树内插件条目自动排除（`EffectiveSourceExcludes`）负责把插件源挡在 app 源发现之外（否则插件
类作为死代码进 app zpkg）。

- 断言①：`build` 成功（插件代建 + 排除同时生效；排除行为另由 z42.project 的 plugin_synth 单测钉住）；
- 断言②：generator 产的符号在 app 产物可用（stdout = 42）。

这是在树 hooks（xtask.hooks / z42.builder.hooks / z42.repl.hooks）迁到单文件条目后的确切形态。
