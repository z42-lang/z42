# plugin-single-file — [plugins] 源码 path 形态（单文件）已删除

`path` 直接指向一个 .z42 源文件曾是免写 z42.toml 的单文件形态；已被源集条目取代
（deps-plugin-routing 收官改硬错）。免清单写单文件插件 → [plugins] 源集条目
（include = 源文件路径，见 plugin-single-file-in-tree——树内自动排也在那条上钉）。

- 断言：build 失败（nonzero）+ stderr 含「源码 path 形态已删除」与改写指引（include）。
