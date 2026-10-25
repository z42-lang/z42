# plugin-dir-no-manifest — `[plugins]` 免清单目录条目

`path` 指向一个**没有工程清单**的目录（plug/：gen.z42 + helper.z42，无 z42.toml）：z42c 合成最小
插件清单（include = `<目录>/**/*.z42` 递归全收）后走与工程条目相同的代建路。

- 断言①：`build` 成功——两个源文件作为一个插件包一起编译（值 42 由 helper.z42 提供：漏收任何一个
  文件即红，钉住「目录全收」语义）；
- 断言②：generator 产的 `E2eOut.O.V()` 在消费方产物里可用（stdout = 42）。

目录里有工程清单时清单优先（plugin 工程 / analyzer-path-entry 等夹具覆盖）；空目录 / 目录里没有
任何 .z42 → 明确报错。要 include-exclude 精细控制 → 写工程清单。
