# 工具链

> 对齐：2026-09-17（change `restructure-docs-three-books`）

编译器与 VM 之外、随 SDK 分发的那些程序：把一个工程推到可运行 / 可分发的东西。

**用法不在这里**——`z42` / `z42c` / `z42b` 的命令面在
[语言与库参考的工具链部分](../../../reference/src/toolchain/README.md)。本部分只写**怎么实现的、在哪改**。

| 页 | 什么时候读 |
|---|---|
| [z42b 构建编排器](z42b.md) | 要改构建/测试/发布的编排阶段，或改 `ICompiler` 注入 |
| [launcher](launcher.md) | 要改 `z42` 的动词分派、SDK 探测、单文件运行的清单合成 |
| [部署形态模型](deployment-model.md) | 要搞清楚 self-contained / apphost / single-file 各自产什么 |
| [`z42 export` 的工程生成](export.md) | 要改 iOS / Android / wasm 工程的生成 |
| [平台 export 与 publish 的动词模型](platform-export.md) | 要加一个平台动词，或搞清楚谁在哪层分叉 |
| [workload 分发](workload-distribution.md) | 要改 workload 的解析、下载、铺设 |
| [REPL](repl.md) | 要改 eval 链路、输入完整性判定、行编辑 |
| [编辑器集成](editor-integration.md) | 要改语法高亮或关键字生成链 |
