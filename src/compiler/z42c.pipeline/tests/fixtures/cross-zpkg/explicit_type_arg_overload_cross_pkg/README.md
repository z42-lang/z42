# explicit_type_arg_overload_cross_pkg

显式类型实参参与重载决议的**跨包**守卫（fix-explicit-type-arg-overload）。

重载集在 `target`（实例 `AddOption(Option)` + `AddOption<T>(string, T, string help = "")`、静态 `Kind`、
自由函数 `Pick`），调用在 `main`（子类构造器里 `this.AddOption<string>("os", "", help: "…")`）。
消费方的候选来自 TSIG 重建的符号：只有导入侧回填了 `TypeParamCount` / `TypeParamNames`，
「只留同 arity 的泛型重载、代换后判适用」才在跨包时成立。
