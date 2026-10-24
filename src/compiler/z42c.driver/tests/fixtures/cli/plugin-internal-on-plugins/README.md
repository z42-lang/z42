# [plugins] 条目的 internal 授权 → 清单错误

internal 授权（同包待遇、宿主 internal 可见）只在 `[plugins.export]` 条目上合法：export 插件跑在
**消费方**编译里、宿主先建，顺序成立。`[plugins]` 条目的插件跑在宿主自己的编译里、必须**先于**宿主
构建——它引用宿主 internal 类型构成环（宿主还没编出来）。断言含改法（挪到 plugins.export）。
