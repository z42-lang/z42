# [plugins] 的 .zpkg path 条目已删除（deps-plugin-routing 收官）

引用现成插件产物统一走 [dependencies]（同一产物读头自动分流）；`[plugins]` 里再写 .zpkg path
条目在清单层直接硬错，错误信息指路 [dependencies]。

- 断言①：插件产物本身仍能单独编出（plug/ 的构建不依赖消费方清单）。
- 断言②：消费方 build 失败（nonzero）+ stderr 同时含「.zpkg path 条目已删除」与「[dependencies]」。

「只换产物内容、消费方必须重编」的指纹面由 deps-plugin-zpkg ③ 钉住（同一路由）。
