# workspace 成员发现：members glob + exclude + 裸名清单

- `members = ["libs/*", "apps/*"]`：按 glob 发现成员。
- `exclude = ["libs/sandbox-*"]`：`libs/sandbox-x` 的源码**故意编不过**，没被剔除构建必红。
- `apps/hello` 用裸名 `z42.toml`（`z42 new` 的形态），并依赖 `wsmcore`（依赖序要对）。
- 两个成员都是 kind=lib：debug 下 exe 装配 indexed lib 的问题由 `bundle-indexed-*` 单独覆盖。
