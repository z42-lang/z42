# z42.compiler.contracts

## 职责

编译与 REPL 的实现中立契约，命名空间保持 `Z42.Build`。定义请求/结果、接口和组件缺失时的空实现；
不包含 Pipeline、进程调度、工作区构建或文件读写实现。`CompileRequest` 携带 project 模型，因此并非 stdlib-only。

## 功能索引

| 入口 | 文件 | 用途 |
|------|------|------|
| ICompiler / CompileRequest / CompileResult / PluginCheck / NoCompiler | `src/ICompiler.z42` | 宿主注入编译与插件检查服务 |
| IReplCompiler / ReplCompileResult / NoReplCompiler | `src/IReplCompiler.z42` | opaque 增量世界、轮编译与补全 |

## 基础用法

宿主通过 `ICompiler` 或 `IReplCompiler` 调用注入的实现；实现位于 `z42c.pipeline`。
请求的 `Manifest` 必填，派生目标与 workspace 域通过既有 body 字段赋值，不改变构造参数列表。

## 如何测试验证

`./xtask test compiler` 构建本库并运行 pipeline 的 `buildsession/contract_boundary_tests.z42`，
验证跨模块动态创建、接口转换和真实编译/REPL 调用；`./xtask test compiler bootstrap` 验证上一 nightly 冷边界。
完整门禁为 `./xtask test`，SDK 部署/搬迁与预编译插件由工具链测试覆盖。

## 依赖关系

消费 `z42.project` 的清单与显式 workspace 域数据。被 `z42.build`、`z42c.pipeline`、`z42.scripting`
及需要编译服务的宿主使用。`z42.build` 只拥有实际编排实现；同一 namespace 不代表相同包归属。

## 关联文档

- [构建编排与注入](../../../docs/internals/src/toolchain/z42b.md)
- [REPL 编译门面](../../../docs/internals/src/toolchain/repl.md)
- [冷自举](../../../docs/internals/src/compiler/self-hosting.md)
