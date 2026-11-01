# runtime/

## 职责

保存每台 VM 的链接定义、身份表、类型组装结果与解析缓存。静态定义来自 metadata 的 PackageImage，对象形状和布局由 object 提供。

## 功能索引

| 入口 | 职责 |
|---|---|
| `Module` / `Function` | 静态定义的链接投影、写时复制和 VM 站点状态 |
| `ids/` | token、FnId / TypeId、函数/类型/字符串表与只追加分段表 |
| `types/` | TypeDesc 注册表组装、继承补全和约束验证 |
| `resolve/` | 每函数预链接与 ResolvedTokens 侧表 |
| `dispatch/ic.rs` | VCall / Field IC、构造站点及原子发布原语 |

## 基础用法

身份表从 `z42::vm::runtime::ids` 导入；运行期 token 位于其 `tokens` 模块。
加载编排调用 types 模块的 `build_type_registry` 和 `verify_constraints`，执行准备调用 `resolve::resolve_module`；加载时序由调用方保持。

## 如何测试验证

仓库根运行 `./xtask test runtime`。单独验证本模块：

```bash
(cd src/runtime && cargo test --workspace vm::runtime:: -- --test-threads=1)
```

## 关联文档

身份的作用域与链接流程见 [VM 总体架构](../../../../../docs/internals/src/runtime/vm-architecture.md)，缓存发布协议见 [内联缓存](../../../../../docs/internals/src/runtime/inline-cache-publication.md)。

## 核心文件

`mod.rs` 汇总链接对象；`function.rs`、`module.rs`、`sites.rs` 与 `fusion.rs` 保存 VM 侧定义和执行计划。子目录的实现及原有测试同目录组织。
