# z42-profiler

## 职责

独立 CPU 采样与堆分析组件，拥有采样线程、栈聚合、堆摘要、快照序列化和保留图。核心只保留诊断契约、计数器和 GC 遍历原语。

## 功能索引

| 入口 | 职责 |
|---|---|
| `src/profiler/component.rs` | 静态和动态共用的组件工厂与同核心身份校验 |
| `src/profiler/cpu.rs` | CPU 采样器和分析服务实现 |
| `src/profiler/heap/` | 堆摘要、V8 快照和保留图算法 |

独立实现通过 `z42::component_abi` 使用描述符和 C 布局接口，通过 `z42::component_bridge` 校验共享核心身份/布局。核心模块仅私有导入，不由本库再导出。

## 基础用法

宿主仅依赖 `z42::host`。动态包按需发现本组件；静态宿主链接本库并登记 `PROFILER_COMPONENT` 与 `z42_profiler_init`。未使用分析功能时不创建实现状态；开启 CPU 采样时执行前装配，堆分析首次调用时装配。缺库、禁用或身份不匹配均报错。

默认浏览器 WASM 不链接本库；需要堆分析时链接同批发布的 `libz42_profiler.a` 并登记工厂。浏览器目标不支持定时线程 CPU 采样。

## 如何测试验证

在 `src/runtime` 运行 `cargo test -p z42-profiler`。完整检查运行 `./xtask test`、`./xtask package sdk --verify` 与真实静态/动态 runtime 包验证。

## 关联文档

- [组件机制](../../../../docs/internals/src/runtime/componentized-runtime.md)
- [宿主门面](../../src/host/README.md)
- [采样与诊断](../../../../docs/internals/src/runtime/diagnostics.md)
