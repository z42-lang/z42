# plugin-single-file — `[plugins]` 单文件条目

`path` 直接指向一个 `.z42` 源文件（plug/gen.z42，**没有** z42.toml）：
z42c 在消费方 cache 目录合成最小插件清单（name = 条目键、kind = "plugin"、include = 源文件绝对路径），
之后走与「插件工程目录」条目完全相同的代建路。

- 断言①：`build` 一步成功——插件被代建、generator（compiler 侧能力位）被加载执行；
- 断言②：generator 产的 `E2eOut.O.V()` 在消费方产物里可用（stdout = 42）。

边界：单文件形态写不出 `[dependencies]`，带依赖的插件（如 z42.harness.lint）仍须完整工程形态——
那条路由 plugin-zpkg-path-entry / analyzer-path-entry 等夹具覆盖。
