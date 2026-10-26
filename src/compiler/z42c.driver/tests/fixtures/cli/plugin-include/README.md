# plugin-include — `[plugins]` 源集条目（plugins-include-form）

`"e2e.set" = { include = ["plug/**/*.z42"], exclude = ["plug/skip/**"] }` —— 与 `[tests]` 同键同语义：
include/exclude 相对消费方工程目录，PluginSynth rebase 成绝对模式合成最小清单后代建。

- 断言①：`build` 绿——两文件一个包（值 42 在 helper.z42，漏收即红），且 `plug/skip/broken.z42`
  被 exclude 挡住（漏进源集 ⇒ 语法错直接红）——**include 与 exclude 同时被钉**；
- 断言②：generator 产的 `E2eOut.O.V()` 在消费方产物可用（stdout = 42）。

树内源集条目的 include 模式自动排除出消费方源发现（显式分区）——由 z42.project 的
plugin_synth 单测钉住。旧免清单形态（`path` 指 `.z42` 文件 / 无清单目录）仍工作、已被源集
形态取代（弃用中）；`path` 指**插件工程**（带清单）不受影响。
