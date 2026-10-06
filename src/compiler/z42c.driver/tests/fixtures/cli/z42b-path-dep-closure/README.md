# `z42b build` 的 path 依赖闭包（repo 外）

z42b 的编排路（`z42b build`，带 rid / workload / hooks 那条）与 z42c driver 共用 `z42.project` 里同一份闭包解析器。
少了它，repo 外对着 `dep = { path = "../foo" }` 的工程连编译都过不了（`E0494: 命名空间 ... 不存在`）。

- **必须在 repo 外跑**：repo 内看着能用，只因依赖早已预建进 libs，看不见差别。
- 两层链：深度为 1 时「直接依赖」恰好等于「闭包」；拷出去跑，`ncleaf` 缺席就会死在 mid 的方法里。
- 产物位置与 z42c 同一份布局答案（单工程默认 `<工程>/dist`）。
- 同一形状的 z42c 侧见 `path-dep-closure`；按名 / 产物引用的闭包见 `dep-zpkg-ref-closure`。
