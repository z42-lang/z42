# exe 引用 SDK 库：默认复制 + 传递闭包

用户的 exe 运行时只有 runtime（runtime 包里没有 SDK 库）⇒ 用到的 SDK 库必须复制进产物，**连同它在 SDK 库内的依赖**：
只声明 `z42c.pipeline`，而它依赖 `z42c.semantics` ⇒ 两个都要在 dist 里。反向：编译器目录里那套 stdlib 副本不能被当私有依赖
拷进来（stdlib 运行期从 `libs/` 解析）。

- 为什么选 `z42c.pipeline` 而不是 `z42.build`：破环预建可能把 `z42.build` / `z42.project` / `z42.package` / `z42c.core` /
  `z42c.syntax` 放进开发树的 stdlib flat，那时它们是「框架」、按规则不复制；`z42c.pipeline` / `z42c.semantics` 从不进 flat，
  才稳定地代表发布态 SDK 里「不在 `libs/` 的 SDK 库」。发布态全量形态由 `xtask package sdk --verify` 覆盖。
- `deploy = "sdk"`（不复制、运行期从 SDK 解析）见 `sdk-lib-deploy-sdk`。
