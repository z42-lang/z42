# config — 运行时配置

## 职责

定义运行时配置、旋钮元数据与解析规则，保留原有读取与缓存时机。

## 功能索引

| 文件 | 职责 |
|---|---|
| `mod.rs` | RuntimeConfig 和全局配置入口 |
| `vm_config.rs` | VmConfig：捕获后的 VM 构造输入 |
| `subsystem_inputs.rs` | 将门面设置转换为 VM/GC/JIT 各自的输入 |
| `subsystem_inputs_tests.rs` | 配置转换、捕获与默认值回归 |
| `knobs.rs / knob_table.rs` | 旋钮类型与注册表 |
| `resolve.rs / source.rs` | 配置来源与优先级 |
| `config_tests.rs` | 配置回归测试 |

## 基础用法

消费者继续从 `z42::config` 使用原有入口。后端支持状态直接查询 `components`，不经宿主门面再导出。

## 如何测试验证

运行 `./xtask test` 与 `./xtask test runtime`，涵盖对应 Rust 测试与端到端行为。

## 关联文档

- [运行时架构](../../../../docs/internals/src/runtime/vm-architecture.md)
- [组件化运行时](../../../../docs/internals/src/runtime/componentized-runtime.md)

## 核心文件

`mod.rs` 是该模块的入口，测试保留在独立文件。
