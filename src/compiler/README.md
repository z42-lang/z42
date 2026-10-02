# z42c — z42 自举编译器（self-host）

## 职责
用 z42 编写的自举编译器：源码全 z42，端到端 `build` 跑通、自编译为 zpkg。z42c 是唯一编译器。编译器域的全部包都在 `src/compiler/` 这一个 workspace 里（`z42.workspace.toml` 为准）：后端三包（semantics / pipeline / driver），以及可移植前端 `z42c.core` / `z42c.syntax`、IR·后端库 `z42.package`、清单模型 `z42.project`、构建管线 `z42.build`、eval 内核 `z42.scripting`。

## 子包（编译器 workspace = 后端三包）
| 子包 → zpkg | kind | 命名空间 | 依赖 |
|------|:----:|------|------|
| `z42c.semantics` | lib | Z42.Semantics（TypeCheck+Codegen）| z42c.core, z42c.syntax, z42.package |
| `z42c.pipeline` | lib | Z42.Pipeline（编排）| z42c.core, z42c.syntax, semantics, z42.package, z42.project |
| `z42c.driver` | **exe** | Z42.Driver（CLI = z42c 入口）| pipeline, z42.package, z42c.core |

**可移植共享库（同在 `src/compiler/`）**：
| 库 → zpkg | 命名空间 | 收敛 |
|------|------|------|
| `z42c.core` | Z42.Core（Span/Diagnostic/Features）| 可移植前端 |
| `z42c.syntax` | Z42.Syntax（Lexer+Parser+AST）| 同上；依赖 z42c.core |
| `z42.package` | Z42.IR + Z42.Package（IR 模型 + zbc/zpkg 后端 + manifest）| IR + 后端 + manifest 合一|

后端三包经**跨-workspace dist 发现**解析这些共享库（冷启动由 `_ensureBootstrapSelfDepLibs` 破环预建，
见 [self-hosting.md](../../docs/internals/src/compiler/self-hosting.md) 轴 ④）。

## 入口点
`z42c.driver.zpkg`（exe）= 用户 `z42c` 命令别名，路由 `build` / manifest-check 等命令（`z42c.driver/src/Main.z42`，含增量构建 `IncrementalDriver.z42`）。

## 构建
```
z42 xtask.zpkg build compiler     # 编译后端 3 包 → artifacts/build/compiler/<pkg>/release/dist/
                                  # （前端 z42c.core/syntax 由 build stdlib 建，先于此进 flat）
z42 xtask.zpkg test  compiler     # 上述 + 断言 3 zpkg 产出（smoke；前端单测归 test stdlib）
```
兄弟依赖经 workspace 自动解析（须在各 manifest `[dependencies]` 声明）；stdlib 自动可用。

## 依赖关系
依赖 stdlib（`src/libraries/`，自动可用）。架构 / 受限写法 / 对账策略见 [docs/internals/src/compiler/self-hosting.md](../../docs/internals/src/compiler/self-hosting.md)。
