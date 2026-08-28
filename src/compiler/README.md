# z42c — z42 自举编译器（self-host）

## 职责
用 z42 重写 C# bootstrap 编译器（`src/compiler/`），到端到端 `build` 跑通 + 与 C# 实现 byte-identical。当前为 **B0 骨架**：7 子包占位 + 构建管线，无真实编译逻辑（Lexer/Parser/... 是 0.3.3 起的后续 spec）。0.3.x 期间 default 编译器仍是 C#，本树两实现并存逐字节对账。

## 子包（编译器 workspace = 后端三包）
| 子包 → zpkg | kind | 命名空间 | 依赖 |
|------|:----:|------|------|
| `z42c.semantics` | lib | Z42.Semantics（TypeCheck+Codegen）| z42c.core, z42c.syntax, z42.ir |
| `z42c.pipeline` | lib | Z42.Pipeline（编排）| z42c.core, z42c.syntax, semantics, z42.ir, z42.project |
| `z42c.driver` | **exe** | Z42.Driver（CLI = z42c 入口）| pipeline, z42.ir, z42c.core |

**已下沉共享库（`src/libraries/`）**——包名/命名空间不变，仍 `z42c.*`：
| 库 → zpkg | 命名空间 | 收敛 |
|------|------|------|
| `z42c.core` | Z42.Core（Span/Diagnostic/Features）| converge-z42-syntax-lib（route A 地基）——可移植前端 |
| `z42c.syntax` | Z42.Syntax（Lexer+Parser+AST）| 同上；依赖 z42c.core |
| `z42.ir` | Z42.IR + Z42.Project（IR 模型 + zbc/zpkg 后端 + manifest）| converge-z42c-ir-metadata（收敛自旧 z42c.ir+z42c.project）|

后端三包经**跨-workspace dist 发现**解析这些共享库（冷启动由 `_ensureBootstrapSelfDepLibs` 破环预建，
见 [self-hosting.md](../../docs/design/compiler/self-hosting.md) 轴 ④）。

## 入口点
`z42c.driver.zpkg`（exe）= 用户 `z42c` 命令别名。B0 骨架仅打印 banner（无桥接）。

## 构建
```
z42 xtask.zpkg build compiler     # 编译后端 3 包 → artifacts/build/compiler/<pkg>/release/dist/
                                  # （前端 z42c.core/syntax 由 build stdlib 建，先于此进 flat）
z42 xtask.zpkg test  compiler     # 上述 + 断言 3 zpkg 产出（smoke；前端单测归 test stdlib）
```
兄弟依赖经 workspace 自动解析（须在各 manifest `[dependencies]` 声明）；stdlib 自动可用。

## 依赖关系
依赖 stdlib（`src/libraries/`，自动可用）。架构 / 受限写法 / 对账策略见 [docs/design/compiler/self-hosting.md](../../docs/design/compiler/self-hosting.md)。
