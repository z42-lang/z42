# z42c — z42 自举编译器

## 职责
用 z42 编写的自举编译器：源码全 z42，端到端 `build` 跑通、自编译为 zpkg。z42c 是唯一编译器。编译器域的全部包都在 `src/compiler/` 这一个 workspace 里（`z42.workspace.toml` 为准，`default-members` 即全部十一个包）。不放用户 stdlib（`Std.*`，在 `src/libraries/`）。

物理位置即「这是编译器域」的声明：普通工程的解析域只有 shipped `libs/`，看不到本 workspace 的包；要用得在 `[dependencies]` 里按名声明。

## 功能索引
| 子包 → zpkg | kind | 命名空间 | 依赖（本 workspace 内）|
|------|:----:|------|------|
| `z42c.core` | lib | `Z42.Core`（Span / Diagnostic / Features）| — |
| `z42c.syntax` | lib | `Z42.Syntax`（Lexer + Parser + AST）| z42c.core |
| `z42.package` | lib | `Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Package`（IR 模型 + zbc/zpkg 读写）| — |
| `z42.project` | lib | `Z42.Project`（`z42.toml` 清单模型）| — |
| `z42.build` | lib | `Z42.Build`（构建管线框架 + `ICompiler` / `IReplCompiler` 接口）| z42.project |
| `z42c.optimization` | lib | `Z42.Optimization`（IR → IR 优化管线 + `Opt` 开关）| z42.package |
| `z42c.semantics` | lib | `Z42.Semantics`（符号收集 + 绑定 + 类型检查 + 校验，不含代码生成）| z42c.core, z42c.syntax, z42.package |
| `z42c.emission` | lib | `Z42.Emission`（代码生成 Bound → IR + 单文件 / 包编译编排 `IrDump`）| z42c.core, z42c.syntax, z42.package, z42c.optimization, z42c.semantics |
| `z42c.pipeline` | lib | `Z42.Pipeline`（编排 / 依赖扫描 / workspace / 增量）| z42c.core, z42c.syntax, z42c.semantics, z42c.emission, z42c.optimization, z42.package, z42.project, z42.build |
| `z42c.driver` | **exe** | `Z42.Driver`（CLI = `z42c` 入口）| z42c.pipeline, z42c.semantics, z42c.emission, z42c.optimization, z42c.syntax, z42c.core, z42.package, z42.project |
| `z42.scripting` | lib | `Std.Scripting`（REPL / 脚本 eval 内核）| z42c.core, z42c.syntax, z42.build |

包间依赖经 `z42c build --workspace` 的拓扑序 + 同 workspace dist 自动发现解析；冷启动由 `_ensureBootstrapSelfDepLibs` 破 z42c ⇄ z42.package 环预建，见 [self-hosting.md](../../docs/internals/src/compiler/self-hosting.md) 轴 ④。

用户入口：`z42c.driver.zpkg`（exe）= `z42c` 命令别名，路由 `build` 等命令（`z42c.driver/src/Main.z42`，增量构建 `IncrementalDriver.z42`）。

## 基础用法
```bash
./xtask build compiler     # 编整个编译器域 → artifacts/build/compiler/<pkg>/<profile>/{dist,cache}/
```

## 如何测试验证
```bash
./xtask test compiler      # z42c 自举不动点 + smoke；跑 tests/<unit>/*.z42.toml 目录单元（semantics / emission / pipeline）
./xtask test stdlib <pkg>  # 扁平 tests/*.z42 的包：z42c.core / z42c.syntax / z42.package / z42.project / z42.build / z42.scripting
```
`<member>/tests/<unit>/*.z42.toml` 是独立 lib 项目，由 `xtask test compiler` 单独构建并运行，不是 workspace member；
`z42c build --workspace` 对其报 WS007（orphan manifest）——非致命、纯提示，可忽略。

## 关联文档
- 架构 / 受限写法 / 对账策略：[self-hosting.md](../../docs/internals/src/compiler/self-hosting.md)
- 编译器总览：[architecture.md](../../docs/internals/src/compiler/architecture.md)

## 核心文件
| 路径 | 职责 |
|------|------|
| `z42.workspace.toml` | workspace 清单：members / default-members / `[workspace.build]` 产物布局 |
| `<pkg>/README.md` | 各子包自己的职责、功能索引与核心文件 |

## 依赖关系
依赖 stdlib（`src/libraries/`，自动可用）。
