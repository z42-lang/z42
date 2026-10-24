# 最小插件清单

`[project]` 只写 name + kind（version 缺省 0.1.0、sources 缺省 src/**/*.z42）；z42c.plugin /
z42c.syntax 契约包与 stdlib 对 kind = "plugin" 免声明。两行清单 + 一个源文件即完整插件工程。

断言走生成符号可用（跑出 7）——清单默认面（version/sources/deps）任何一处不生效都会红。
