# Tasks: 单工程 `z42c build --output-dir` 不再往工程目录旁写 cache / generated

**状态：🟡 进行中 | 开始：2026-09-30**

类型：`fix`（z42c，用户可见：`--output-dir` 的辅助输出落点）→ 最小化模式。「源码树零写入」系列第 ③ 步。

## 问题

`z42c build --help` 说 `--output-dir <dir>` 会 "put all outputs in <dir>"，实际上单工程下只有 dist 进 `<dir>`：
- 增量 cache 仍按 manifest 解析写到 `<proj>/artifacts/<profile>/.cache` —— 而且这条路径**从不 probe**
  （Main.z42 的 workspace-read-cache 注释），写了永远不读；
- generator 生成的源码仍写到 `<proj>/artifacts/<profile>/generated`。

后果：xtask 自举预建 6 个自依赖库（`_ensureBootstrapSelfDepLibs`，每次 `build compiler` 都跑）、以及所有
用 `--output-dir` 编测试夹具的地方，每轮都在源码树里留下产物目录。

## 方案（与最初口头方案的差异）

最初设想是 cache 改写到 `<dir>/.cache/<name>`（照 flat workspace 的约定）。调研发现自举预建的 `<dir>` 就是
stdlib flat（= `Z42_LIBS` = 打进 SDK 的 `libs/`），往里放 cache 目录有被整目录拷进包的风险；而这条路径本就
不读 cache。故改为：

- **单工程 `--output-dir`：不写 cache**（`writeCache`）。indexed dist 的「cached 字节原样复制」只在增量开启时
  消费，本路径不开，不受影响。
- **generated → `<dir>/generated/<name>`**（它是调试用的，保留）；显式 `generated_dir = ""` 仍不落盘。
- workspace 构建（per-member / flat，`tier != null`）行为不变；它们的 generated 仍按成员 manifest 解析，
  归入「统一默认布局」那一步。

## 进度概览

- [x] Main.z42：`writeCache` + generated 落点
- [x] BuildCommand.z42 usage；BuildCache.z42 注释
- [x] 文档：cli-z42c-z42b.md、cli-z42.md
- [x] e2e：xtask_compiler_e2e_cache.z42 新增 ④（产物进 `<dir>`、工程目录旁零写入）
- [x] 完整 GREEN（`build all` + 新 driver 自建 gen2 后跑 `xtask test`，9m48s，全阶段通过；e2e ④ 通过；stdlib / 编译器成员目录下自举预建留下的 `artifacts/release/.cache` 消失）
- [ ] PR CI + 归档

## 自举影响

不碰 zbc / zpkg 格式、不引入新语法。种子 z42c 执行的路径（冷启动 `_ensureBootstrapSelfDepLibs`、
`test bootstrap`、CI 编 xtask）要等下一个 nightly 才有新行为 —— 期间照旧写 cache，无害（滞后，不会坏）。
