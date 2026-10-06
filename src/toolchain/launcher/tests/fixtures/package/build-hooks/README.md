# build-hooks

`[build] hooks` 的 hook 类要继承 `Z42.Build.BuildHooks`，而 `z42.build` 不在 SDK 的 `libs/`，在编译器目录
（`programs/z42c/`）。z42b 编 hooks 时若只给 `Z42_LIBS`，装好的 SDK 里 hooks 一律编不过
（`E0494: 命名空间 Z42.Build 不存在` / `E0443: undefined type: BuildHooks`）。

开发树那格（`xtask test toolchain builder`）测不到这条：那里的解析域与发布态不同。这里用打包出的 SDK、
并清掉全部 z42 相关环境变量（`z42b` 工具的缺省环境），就是真实用户的样子。
