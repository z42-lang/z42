# 复制判据：从 `Z42_LIBS` 解析到的框架包不进 exe dist（repo 外）

`z42c build` 判「依赖要不要复制进 exe 的 dist」看**它是从哪个目录找到的**：从编译期外部档（`Z42_LIBS`）找到的是框架，
不复制。外部档与解析面是同一份（driver 的 `_extLibDirs()`）。

- 自举时上一代 driver 的运行期库挂在 `Z42_PROBING_PATHS` 上、目录不同于 `Z42_LIBS`。判据若按「driver 运行期从哪加载」
  而不是「编译期从哪解析」来判，从 `Z42_LIBS` 解析到的 stdlib 会全被当成私有包复制，z42c.driver 的 dist 随之带上整套
  stdlib，经 publish 进 SDK 的 `programs/z42c/`。本用例让两个目录内容相同、路径不同来守住这一点。
- 传递依赖要单独覆盖：`cfapp` 自己不声明任何 stdlib，`z42.text` 是复制 `cfdep` 时从它的 DEPS 段带进来的
  （z42c.driver 的 stdlib 副本大多也是这么来的：编译器兄弟包的 DEPS）。
