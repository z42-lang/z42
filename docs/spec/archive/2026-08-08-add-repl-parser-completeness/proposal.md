# Proposal: REPL parser 权威的输入完整性判定

## Why

REPL 现在靠 native `bracket_depth`（纯括号计数）判断"输入是否写完、要不要续读下一行"。这是完整性判定的一个**词法子集**——只能抓"括号没配平"，抓不到"没有括号但同样没写完"的输入：

```
>>> class B
第 2 行: E0202: expected '{'
第 2 行: E0202: expected '}'
>>> {
```

`class B` 没有任何括号 → `bracket_depth == 0` → 被当作"写完了"直接送去编译 → 报 `expected '{'`。同类漏判还有 `void foo()`（缺正文）、`1 +`（缺操作数）、`if (x > 0) {`（其实这个有括号能抓到，但 `if (x)` 之后接 `{` 前的空档不行）。

根因：**"输入是否完整"的唯一权威应该是 parser（它才知道语法上还缺什么），而不是括号计数这种词法近似。** 主流 C 系 REPL（C# Roslyn `IsCompleteSubmission`、Node `isRecoverableError`）都让 parser 来回答。括号法每加一种新语法就要回头补一条续行规则，是一处会随语言演进持续漂移的双份逻辑。

## What Changes

- **parser 侧**：新增"读到 EOF 还缺 token"的**可恢复不完整**信号（`IncompleteAtEof` 标志位），在缺 token 且当前 token 已是 EOF 的报错点置位。复用已定义但从未使用的诊断码 `E0203 UnexpectedEof` 语义。
- **脚本层**：新增 `Std.Scripting.Completeness.IsIncomplete(src)` 探针——对**裸输入原文** parse（不包裹、不执行），读 parser 的 `IncompleteAtEof` 判定是否续读。判定与求值（`Script.Eval`）彻底解耦。
- **REPL 主循环**：多行累积逻辑从 native `ReadBlock` 上移到脚本层——逐行 `ReadLine` → 累积 → `Completeness.IsIncomplete` 判续读/求值。续行保留自动缩进（native 括号法算缩进，纯视觉装饰）。续行途中 Ctrl-C 放弃当前缓冲、回到主提示符。
- **native**：`ReadLine` 已够用；新增 `ReadLineIndented(prompt, buf)`（据已累积 buf 算缩进预填后读单行）；按"不留兼容"删 `__repl_readblock` / `Repl.ReadBlock`。

**关键设计约束（为什么不能试编译包裹后的代码）**：z42 REPL 输入执行时会被包进 `return (…)` / 函数体（z42 无顶层语句语义）。包裹尾部的 `)` `}` `;` 会"接住"本该落到 EOF 的缺失点，使 `IncompleteAtEof` 永不置位。因此完整性判定**必须对裸原文 parse**，与"执行时的包裹编译"分成两条路。详见 design.md。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.core/src/DiagnosticBag.z42` | MODIFY | 加 `bool IncompleteAtEof` 标志位 + 置位/读取方法 |
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | `_expect` else 分支：`_peek()==Eof` 时置 `IncompleteAtEof`；暴露 `_diags` 的读取 |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | 缺操作数 fallthrough：`_peek()==Eof` 时置位（覆盖 `1 +`） |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | 名字位置内联报错（`expected type name` 等）：`_peek()==Eof` 时置位（覆盖 `class` 后即 EOF） |
| `src/toolchain/scripting/src/Completeness.z42` | NEW | `IsIncomplete(string src) -> bool` 探针（Classifier 分派 + 现有 parser 入口） |
| `src/toolchain/scripting/src/Repl.z42` | MODIFY | 加 `ReadLineIndented(prompt, buf)` binding；删 `ReadBlock` |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 主循环多行累积上移 + Ctrl-C 逃生；`-c` 单次路径拿到 incomplete 当语法错误 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | 加 `__repl_readline_indented`；删 `__repl_readblock`；保留 `bracket_depth`/`continuation_indent`（供缩进） |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `__repl_readline_indented`、注销 `__repl_readblock` |
| `src/runtime/src/gc/safepoint.rs` | MODIFY | `NativeParkGuard` 注释中 readblock → readline_indented |
| `src/runtime/src/corelib/repl_tests.rs` | （未改）| `bracket_depth`/`continuation_indent` 函数保留 → 现有测试仍有效，无需改动 |
| `src/compiler/z42c.syntax/tests/parser/incomplete_at_eof_tests.z42` | NEW | parser `IncompleteAtEof` 各场景单测（加入现有 parser/ 测试包，复用 toml） |
| `src/toolchain/scripting/tests/completeness/` | NEW | `Completeness.IsIncomplete` driver e2e（driver.z42 + expected_output.txt） |
| `docs/book/src/toolchain/repl-input-completeness.md` | NEW | REPL 完整性判定机制（parser 权威 + 探针解耦 + 裸 parse）；挂 `SUMMARY.md` |
| `docs/book/src/SUMMARY.md` | MODIFY | 工具链节挂新 book 页 |
| `docs/book/src/compiler/error-codes.md` | MODIFY | `E0203` 兼作 REPL 可恢复不完整信号说明 |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引 + 核心文件加 `Completeness.z42` |
| `src/toolchain/interactive/README.md` | MODIFY | `interactive_main` 续行/多行机制说明更新 |

**只读引用**（理解上下文必须读，不修改）：

- `src/toolchain/scripting/src/Classifier.z42` — 复用其 `IsDecl` 分派声明 vs 表达式/语句
- `src/toolchain/scripting/src/Script.z42` — 理解 Eval 包裹编译路径（确认其不受影响）
- `src/toolchain/scripting/src/EvalResult.z42` — 确认无需改（判定在 Eval 之前）
- `src/compiler/z42c.syntax/src/MemberParser.z42` / `StmtParser.z42` — 理解 `void foo()`（`_expectSemi`）与块体缺 `}` 的报错路径
- `src/compiler/z42c.core/src/DiagnosticCodes.z42` — `E0203 UnexpectedEof` 码位

## Out of Scope

- **泛型返回类型的函数头续行**（`List<int> foo()`）：Classifier 保守漏判（`<>` 非括号）→ 当表达式路径 → 报错而非续读。已知边界，记 Deferred。
- **空行强制结束多行块**：z42 靠 `{}`/操作数补全显式结束，不需要 Python 式"空行结束缩进块"机制。
- **`.` 元指令的多行**：元指令单行即完整，不参与续行判定。
- **将 `IncompleteAtEof` 透传到 `CompiledModuleZ`/`CompileArtifacts`/`EvalResult`**：探针只 parse 不走完整编译管线，无需顶层透传（这是"探针解耦"架构的直接收益）。

## Open Questions

- [x] Ctrl-C（Interrupted）与 Ctrl-D（Eof）在 native `read_one_line` 层是否可区分？**已核实**：现有 `read_one_line`（repl.rs:539）把两者都归一为 `Ok(Value::Null)`。**定稿方案**（无需改 native 契约）：主循环用「buf 是否为空」定语义——续读中（buf 非空）收 `null` → 放弃当前多行缓冲、回主提示符；主提示符（buf 空）→ 退出。见 design.md Decision 5。
