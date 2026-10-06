# `deploy` 声明的校验对所有 kind 生效

`deploy` 的校验在清单层（`_build` 开头），不在只有 exe 才跑的依赖装配里。否则三格都是同一个形状 ——
**配了，没报错，就是不生效**：

- `kind = "lib"` 里写 `deploy = "Copy"`（大写 typo）；
- `kind = "analyzer"` 的依赖写任何 `deploy`（编译期扩展永不部署）；
- `[analyzers]` 条目写 `deploy`（它与 `[dependencies]` 共用条目类型，键写得出来）。

exe 的非法取值在 `deploy-key-overrides` ④；这里刻意只盖非-exe 那半，免得两个用例守同一件事。
