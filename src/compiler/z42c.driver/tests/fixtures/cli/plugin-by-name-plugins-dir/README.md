# [plugins] 按名条目已删除（deps-plugin-routing 收官）

按名引用插件产物统一走 [dependencies]（读 zpkg 头自述类型自动分流）；`[plugins]` 里再写按名条目
（`"x" = "0.1.0"`）在清单层直接硬错，错误信息指路 [dependencies]。按名的插件目录解析语义
（Z42_PLUGINS 查找域分离）活在 deps 侧——正路径与阴性见 deps-plugin-by-name。

- 断言：build 失败（nonzero）+ stderr 同时含「按名条目已删除」与「[dependencies]」。
