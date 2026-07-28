# Tasks: REPL MVP 元指令补齐

> 状态：✅ 已归档 2026-07-28（PR #53 合并 a139cb8e）| 创建：2026-07-28 | 占用子系统：`toolchain` + `stdlib`

## 进度
- [x] scripting 库：`Classifier` 加 `IsTypeDecl`；`ScriptState` 加 `DeclTypeNames`；`Script` 记类型名 + `FormatVersion()`
- [x] 宿主：`.reset`（Counter 保持单调）/`.clear`（`(char)27`）/`.using <ns>`/`.types`/`.version` + `_help`
- [x] 文档：`repl.md` 元指令落地状态刷新（消除 doc-vs-code 冲突）+ scripting README 功能索引
- [x] 测试：扩 `repl_decls_multiline` 驱动断言 `DeclTypeNames`（`.types` 数据源；类型 Adder/Color，排除自由函数）
- [x] 验证：warm 手动构建 scripting + interactive → `-c "1+2"=3`；管道会话逐指令实测（见下表）
- [x] commit + push（92ed3121）→ PR #53 → 合并 a139cb8e，CI GREEN

## 本地验证（管道 stdin 实测）
| 指令 | 结果 |
|------|------|
| `.version` | `zbc 1.28, zpkg 0.33` ✓ |
| `.vars` / `.types` | 变量 `x`；类型仅 `Point`（排除函数 `dbl`）✓ |
| `.using Std.Collections` → `.usings` | 追加 + 列出 ✓ |
| `.reset` → 再求值 `100+23`=`123` | 清空 + **Counter 单调防撞名** ✓ |
| `.clear`（非终端） | no-op 不崩 ✓ |
| `.help` / `.badcmd` / 多行 `add()` | 全指令列出 / unknown / 续读=7 ✓ |
| `repl_decls_multiline`（扩展后）| PASS ✓ |

## 备注
- `.version` 仅打印 zbc/zpkg **格式**版本；z42vm 运行时版本串需新 builtin → follow-up `repl-future-runtime-version`。
- `.history`/`.save`/`.mode` 仍未接（需 transcript / ExecMode 基建）。
