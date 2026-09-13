# Tasks: REPL 把 Z42_LIBS 按 `:` 拆成多目录，与 VM 运行期口径不一致

> 状态：🟢 已完成 | 完成：2026-09-13
> 变更类型：`fix`（最小化模式）

**变更说明：** `Z42_LIBS` 是**单**目录（共享 `libs/`：stdlib + 预建包）—— z42c driver、z42b、
VM 运行期（`startup.rs::resolve_libs_dir`）都只当一个目录用，`docs/book/src/compiler/project-build.md`
也明写「不是 PATH 式多段」。只有 REPL 的两处按 `:` 拆：
`Script.Create`（编译依赖目录）与 `ReplCompilerHost._findCompilerZpkg` 探测序 ④。

**原因 / 症状：** 两侧口径不一致 ⇒ 放在第二段目录里的库**编译期看得见、运行期加载不到**：
会话第一句 `Greeter.Hi()` 就报 `MissingSymbolException: undefined function MyLib.Greeter.Hi$0`
（运行期、费解），而不是编译期的 `E0401`。另外按 `:` 拆会把 Windows 的 `C:\...` 拆成两段。

**口径裁决（User，2026-09-13）：** Z42_LIBS 指标准库路径，只保留一个 ⇒ 收窄 REPL 侧，不扩 VM 侧。
`Script.CreateWithLibs(libsCsv)` 是显式 API（wasm playground 传 VFS 目录），不读环境变量，不在本次范围。

**文档影响：** 无（`project-build.md` 早已写明单目录；本次是让 REPL 对齐文档）。

- [x] 1.1 `Script.Create`：`Z42_LIBS` 整串作为唯一依赖目录，不再 `Split(":")`
- [x] 1.2 `ReplCompilerHost._findCompilerZpkg` ④：只探测 `Z42_LIBS/z42c.pipeline.zpkg`
- [x] 1.3 端到端对照（`z42i`，独立 cwd 防止 VM 回退到开发树 `artifacts/build/libraries/dist/release`
      —— 回退会让「旧版」实际加载到新 `z42.scripting`，第一次对照就是这么作废的）：

      | Z42_LIBS | 修复前 | 修复后 |
      |---|---|---|
      | `libs:libdir`（mylib 在第二段） | `MissingSymbolException: undefined function MyLib.Greeter.Hi$0` | `E0401: undefined: Greeter` |
      | `libs`（mylib 在其中） | `v1` | `v1` |

      z42.scripting 的 `tests/` 是 REPL golden 驱动、`[tests] auto = false` 未接任何运行器，故无自动化回归可加。
- [x] 1.4 GREEN：`xtask test` 全 stage 绿
