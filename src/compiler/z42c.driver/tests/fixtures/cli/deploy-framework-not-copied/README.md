# 复制判据：框架目录里的包不进 exe dist（repo 外）

`z42c build` 决定「这个依赖要不要复制进 exe 的 dist」的判据是「**它是从哪个目录找到的**」：从 shipped
`libs/`（`Z42_LIBS`）找到的是框架，不复制；其余复制。这是「这台机器上的框架在哪」这个运行期事实，repo 内外都成立。

- **用例必须在 repo 外跑**（`outside_repo = true`）：repo 内 `_srcRoot` 找得到仓库根，走的是「`<srcRoot>/libraries/<name>`
  存在」那条目录判据，按名字前缀猜的缺陷在那里看不见；用户机器上 `_srcRoot` 恒为空，判据会退化成
  `dep.StartsWith("z42.")`。`deployfx` 故意**不叫** `z42.*`。
- 判据也不能写成「这个名字在 `Z42_LIBS` 里存在吗」：构建期 `Z42_LIBS` 若混装了非框架包（stdlib + 编译器成员），
  `z42c.pipeline` 会被判成框架而不复制，`z42c.driver` 的自包含 bundle 就缺件。那一半由 `test compiler` 的自举
  与 CI 的 bench A/B 守着（本地 `Z42_LIBS` 是干净的 stdlib flat，看不见），本用例只守「repo 外不按名字前缀猜」。
