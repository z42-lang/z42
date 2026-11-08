# plugin-single-file-in-tree — 源集插件住消费方树内

源集条目 `include = ["igen/igen.z42"]` 圈中的源文件住在 app 树**内**，且 app 的
`[sources].include = ["**/*.z42"]` 会匹配到它——源集条目显式分区负责把插件源挡在 app 源发现之外
（模式圈中的文件归插件，不被扫进 app 产物；否则插件类作为死代码进 app zpkg）。

- 断言①：`build` 成功（插件代建 + 排除同时生效；排除行为另由 z42.project 的 plugin_synth 单测钉住）；
- 断言②：generator 产的符号在 app 产物可用（stdout = 42）。

这是在树 hooks（xtask.hooks / z42.builder.hooks / z42.repl.hooks）的现行形态（源集条目）。
