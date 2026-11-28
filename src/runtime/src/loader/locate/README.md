# locate — 包定位与搜索路径机制

## 职责

按调用者提供的目录解析 namespace/zpkg，展开显式 probing 模式。环境和默认目录策略由宿主来源实现决定。

## 功能索引

- `namespace.rs` / `namespace_index.rs`：依赖和 namespace 候选查询，旧版 ZBC 的反查在一次加载中共享同一个候选快照。
- `search_paths.rs`：由显式入口、额外路径和 libs 构造稳定去重的目录序列，使用传入 VFS 检查入口目录；不读环境。
- `probing.rs`：路径模式、SDK 占位符与缺失提示；纯展开入口接受显式根，进程便利入口供来源适配调用。

## 基础用法

main 等来源实现先解析配置/环境，再构造搜索路径交给 `host::LoadOptions`；加载接口不反向查询 CLI。

## 如何测试验证

仓库根运行 `./xtask test runtime` 与完整 `./xtask test`。路径顺序、去重、空输入及 MemoryFs 在同目录测试中验证。

## 关联文档

- [模块边界](../../../../../docs/internals/src/runtime/module-boundaries.md)
- [VM 架构](../../../../../docs/internals/src/runtime/vm-architecture.md)

## 核心文件

`mod.rs` 声明查询与路径模块；`search_paths_tests.rs` 验证显式路径契约，`probing_tests.rs` 验证模式展开。
