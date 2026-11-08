# plugin-dir-no-manifest — [plugins] 源码 path 形态（无清单目录）已删除

`path` 指向没有工程清单的源目录曾是免清单目录形态（递归全收合成清单）；已被源集条目取代
（deps-plugin-routing 收官改硬错）。要免清单写插件 → [plugins] 源集条目
（include 目录 glob，对齐 [tests]，见 plugin-include）；目录里有工程清单的 path 条目不受影响。

- 断言：build 失败（nonzero）+ stderr 含「源码 path 形态已删除」与改写指引（include）。
