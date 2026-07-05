# z42c — z42 自举编译器（self-host）

## 职责
用 z42 重写 C# bootstrap 编译器（`src/compiler/`），到端到端 `build` 跑通 + 与 C# 实现 byte-identical。当前为 **B0 骨架**：7 子包占位 + 构建管线，无真实编译逻辑（Lexer/Parser/... 是 0.3.3 起的后续 spec）。0.3.x 期间 default 编译器仍是 C#，本树两实现并存逐字节对账。

## 子包（独立 workspace，依赖序）
| 子包 → zpkg | kind | 镜像 C# | 依赖 |
|------|:----:|------|------|
| `z42c.core` | lib | z42.Core（Span/Diagnostic/Features）| — |
| `z42c.ir` | lib | z42.IR（IR 模型 + zbc + 项目类型）| — |
| `z42c.syntax` | lib | z42.Syntax（Lexer+Parser+AST）| core |
| `z42c.project` | lib | z42.Project（manifest reader）| ir |
| `z42c.semantics` | lib | z42.Semantics（TypeCheck+Codegen）| core, syntax, ir |
| `z42c.pipeline` | lib | z42.Pipeline（编排）| core, syntax, semantics, ir, project |
| `z42c.driver` | **exe** | z42.Driver（CLI = z42c 入口）| pipeline, ir, core |

## 入口点
`z42c.driver.zpkg`（exe）= 用户 `z42c` 命令别名。B0 骨架仅打印 banner（无桥接）。

## 构建
```
z42 xtask.zpkg build compiler     # 编译 7 子包 → artifacts/build/compiler/<pkg>/release/dist/
z42 xtask.zpkg test  compiler     # 上述 + 断言 7 zpkg 产出（smoke；opt-in soak）
```
兄弟依赖经 workspace 自动解析（须在各 manifest `[dependencies]` 声明）；stdlib 自动可用。

## 依赖关系
依赖 stdlib（`src/libraries/`，自动可用）。架构 / 受限写法 / 对账策略见 [docs/design/compiler/self-hosting.md](../../docs/design/compiler/self-hosting.md)。
