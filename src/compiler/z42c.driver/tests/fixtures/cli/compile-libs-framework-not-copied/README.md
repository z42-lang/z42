# 复制判据：`--compile-libs` 下框架包不进 exe dist（repo 外）

`z42c build` 判「依赖要不要复制进 exe 的 dist」看**它是从哪个目录找到的**：从编译期外部档（`--compile-libs`，未给则
`Z42_LIBS`）找到的是框架，不复制。外部档与解析面是同一份（`CompileLibsOpt.ExtDirs()`）。

- 只认 `Z42_LIBS` 时，自举（`Z42_LIBS` = 种子代 run-libs、`--compile-libs` = flat）从 flat 解析到的 stdlib 全被当成
  私有包复制，z42c.driver 的 dist 随之带上整套 stdlib，经 publish 进 SDK 的 `programs/z42c/`。
- `run/` 与 `libs/` 内容相同、目录不同：判据比的是目录，不是名字在不在。
- 传递依赖要单独覆盖：`cfapp` 自己不声明任何 stdlib，`z42.text` 是复制 `cfdep` 时从它的 DEPS 段带进来的
  （z42c.driver 的 stdlib 副本大多也是这么来的：编译器兄弟包的 DEPS）。
