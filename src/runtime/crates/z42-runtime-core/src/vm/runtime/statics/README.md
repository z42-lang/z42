# statics/

## 职责

保存每 VM 的静态字段单元与类型初始化事实。执行、等待及异常构造由 `vm/exec/init.rs` 负责。

## 功能索引

| 功能 | 入口 / 文件 |
|---|---|
| 静态字段编号、原子单元与 GC 根 | `table.rs` 的 `StaticTable` / `StaticCell` |
| 初始化状态、包归属、代际与屏障纪元 | `cctor.rs` 的 `CctorRegistry` |

## 基础用法

内部通过 `vm::runtime::statics` 访问存储与查询。初始化执行层使用同一份 registry，
在 `vm::exec::init::claim_with` 等待时提供 native park 守卫；数据层不等待或调用 VM 代码。

## 如何测试验证

运行 `./xtask test runtime`；定向运行：

```bash
(cd src/runtime && cargo test --workspace vm::runtime::statics::)
```

已有测试覆盖并发发布、GC 根、失败终态、重入与 parked 等待。

## 关联文档

- [静态初始化机制](../../../../../../../../docs/internals/src/runtime/static-ctor-init.md)
- [VM 架构](../../../../../../../../docs/internals/src/runtime/vm-architecture.md)

## 核心文件

| 文件 | 职责 |
|---|---|
| `table.rs` / `table_tests.rs` | 稳定单元、属主与纪元缓存及并发 GC 测试 |
| `cctor.rs` / `cctor_tests.rs` | 登记状态、包归属、发布规则及初始化契约测试 |
