# ws-redundant-member-dep

retire-member-dep-decls 的行为样本：

- **同 workspace 成员依赖自动解析**——`wsapp` 能用 `WsLib`，靠的是成员 dist 进 `libsDirs`（WsTier 分档），
  不是 `[dependencies]` 里那条 `"wslib" = "0.1.0"`。
- **WS032**——成员清单按名声明同 workspace 成员（无 `path` / `deploy` 键）⇒ 纯提示、构建照常；
  `z42c build --workspace` 与 z42b 的 workspace 动词都会逐条打在 stderr。
- **构建序 = `default_members` 声明序**——两个成员名字序是 `wsapp < wslib`，但 app 依赖 lib；
  构建能成、产物能跑，说明排序听声明序（`WorkspaceMemberDeps.ReadyOrder`），不是名字序。
