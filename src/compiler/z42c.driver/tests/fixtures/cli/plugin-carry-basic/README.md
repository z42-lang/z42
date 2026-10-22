# [plugins.export] 携带链

lib 用 `[plugins.export]` 声明「依赖我的工程自动获得这些插件」——app 只写 `[dependencies]`，
不写任何 `[plugins]`，generator 照样生效（③ 跑出 42）。携带在 lib 构建时写进 zpkg 的 PLGS 段
（名 + 精确版本），消费方沿**真闭包**（声明序 BFS）读到并入；按名在插件目录（`Z42_PLUGINS`）解析。

- ④ 是关闭语义的判别力断言：`"e2e.cgen" = false` 后 generator 不跑，`E2eOut` 符号不存在——
  构建必须红在「符号未定义」上，而不是静默跳过 generator 后编出旧结果。
