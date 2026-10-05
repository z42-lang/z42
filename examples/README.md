# examples — 《z42 学习手册》配套示例

这里的每个章节目录对应[学习手册](https://z42-lang.github.io/z42/learn/)的一章，里面是书中用到的完整代码，可以直接运行：

```sh
cd examples/getting-started/hello-world/greet
z42 run greet.z42 -- 小明
```

| 目录 | 章节 |
|------|------|
| `getting-started/` | [入门](../docs/learn/src/getting-started/)：`hello-world/`、`projects/`、`tooling/` |
| `basics/` | [基础](../docs/learn/src/basics/)：`variables` / `operators` / `strings` / `control-flow` / `functions` / `collections` / `tuples` |
| `types/` | [类型](../docs/learn/src/types/)：`classes` / `structs-records` / `inheritance` / `interfaces` / `generics` / `lambdas` / `exceptions` / `patterns` / `attributes-reflection` / `organization` |
| `stdlib/` | [标准库](../docs/learn/src/stdlib/)：`data-formats` / `files` |

## 给贡献者

- 目录结构与页面路径一一对应：`examples/<part>/<chapter>/` ↔ `docs/learn/src/<part>/<chapter>.md`。
- 书里的代码和终端输出全部 include 自这里；`*.console` 是会话脚本，记录命令与期望输出。
- 这里只放手册配套内容；语言与库特性的测试写在 `src/tests/` 或各库的 `tests/`。
- 校验：`xtask test examples`（需要先 `xtask build sdk`）。写法与规则见
  [`docs/agent/rules/learn-writing.md`](../docs/agent/rules/learn-writing.md)。
