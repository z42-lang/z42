# 按名 / 产物引用的传递闭包

`app → mid → leaf` 时 `leaf` 也得随产物走。只搬直接依赖的话，`dist/` 里只有 `ncmid.zpkg`，拷到别处运行会死在 mid 的方法里
（`MissingSymbolException: type NcLeaf.Deep`）—— 症状离原因很远，看起来是「mid 的代码有问题」，实际是打包漏了 leaf。

- 间接依赖的名字从 **zpkg 的 DEPS 段**读，不是源码树的清单 —— 那条路在 repo 外整个失效，所以用例在 repo 外跑。
- path 依赖的闭包见 `path-dep-closure`（z42c）与 `z42b-path-dep-closure`（z42b）。
- `source-app → source-mid → vendor/ncmid.zpkg → vendor/ncleaf.zpkg` 验证源码成员的解析域随部署边传递。
  用例运行独立产物、删除 vendored 副本后走 preserved 重建，再检查不同来源同名包的诊断。切换为 `shared` 后确认旧包和符号副本被移除，手工替换的符号文件仍保留。
