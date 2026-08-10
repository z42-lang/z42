# Design: REPL parser 权威的输入完整性判定

## Architecture

核心是把"输入完整性判定"和"求值执行"**解耦成两条独立的路**，且完整性判定对**裸原文**做（不包裹）：

```
                    ┌─────────────── REPL 主循环（脚本层 interactive_main）───────────────┐
                    │                                                                       │
   Repl.ReadLine ──▶│  buf += line                                                          │
   (native 单行读)   │       │                                                                │
                    │       ▼                                                                │
                    │  Completeness.IsIncomplete(buf)  ──── 完整性探针（裸 parse，不执行）    │
                    │       │  Classifier.Classify(buf) 分派：                               │
                    │       │    IsDecl → Parser.ParseCompilationUnit(buf)                   │
                    │       │    else   → Parser.ParseStatement(buf)                         │
                    │       │  读 parser.Diags.IncompleteAtEof                               │
                    │       │                                                                │
                    │   true│（不完整）          false│（完整）                              │
                    │       ▼                        ▼                                       │
                    │  ReadLineIndented("... ",buf)   Script.Eval(state, buf) ── 求值路 ─────┐│
                    │  （native 据 buf 算缩进预填）        │  包裹 return(…)/函数体             ││
                    │       │ 续读追加，回到探针            │  → PackageCompile → 执行           ││
                    │       └───────────────┘             │  → EvalResult（不改）             ││
                    └────────────────────────────────────┴────────────────────────────────┘│
                                                                                             │
   两条路唯一共享：Classifier（声明 vs 表达式/语句 分类）。完整性探针不走 PackageCompile，       │
   不触碰 EvalResult / CompiledModuleZ / CompileArtifacts —— 信号只需挂在 parser 的 Diags 上。  │
```

## Decisions

### Decision 1: 完整性判定必须对裸原文 parse，不能试编译包裹后的代码

**问题：** 直觉上 B 方案 = "把输入喂编译器，报 EOF 不完整就续读"。但 z42 REPL 执行时把输入包进 `return (…)` / 函数体（z42 无顶层语句语义）。

**冲突：** 包裹尾部的 `)` `}` `;` 会"接住"本该落到 EOF 的缺失点。例：`1 +` 包成 `return (1 +);` → parser 缺右操作数时，下一个 token 是 `)` **不是 EOF** → `IncompleteAtEof` 永不置位 → 判不出续读。

**决定：** 完整性判定对**裸输入原文** parse（`Completeness.IsIncomplete` 直接 `new Parser(src)`，不加任何包裹），与求值路（`Script.Eval` 内部照旧包裹）分成两条路。

**旁证（其他语言）：** Python `codeop` / C# Roslyn `IsCompleteSubmission` 之所以能 work，正因它们 parse 的是输入原文——那些语言的 REPL 输入本身就是合法编译单元。z42 一旦包裹就失去这个前提，故必须裸 parse。

### Decision 2: 判定与执行解耦，`EvalResult` 不改、信号不透传 pipeline

**问题：** 另一种实现是让 `Script.Eval` 试编译、返回 `NeedMoreInput` 状态。

**选项：**
- A（探针解耦）：续读判定在 `Eval` **之前**独立做；`Eval` 只在"已完整"时被调。
- B（Eval 内联）：`Eval` 试编译，把 `IncompleteAtEof` 从 parser 一路透传到 `CompiledModuleZ → CompileArtifacts → EvalResult.NeedMoreInput`。

**决定：** 选 A。理由：
1. **改动面更小**：信号只需挂在 parser 的 `DiagnosticBag` 上，不用改 `CompiledModuleZ`（IrDump.z42）/ `CompileArtifacts`（PackageCompile.z42）/ `EvalResult` 的结构与语义。
2. **避开两次编译坑**：`Eval` 对表达式输入是"表达式优先 `return(…)` 失败则回退语句 `…;return null`"两次编译；内联判定要小心 incomplete 信号被第二次编译的其它错误盖掉。探针在 Eval 之前一次裸 parse，无此问题。
3. **单一职责**：完整性判定（只 parse）与求值（编译+执行）关注点分离。
4. **轻量**：探针不加载依赖、不 warm worker、不执行，只 parse。

### Decision 3: `IncompleteAtEof` 挂 `DiagnosticBag`，按"报错点当前 token == EOF"统一置位

**问题：** 哪些报错点、用什么判据置 `IncompleteAtEof`？

**坑：** 不能只认"`E0202 expected '{'` 且在 EOF"。因为 `void foo()`＋EOF 报的是 `expected ';'`（无体函数 `void foo();` 合法，`_expectSemi` 触发），`1 +` 报的是 `E0201 unexpected token`（不是 E0202）。**统一判据必须是"报错时 `_peek().Kind == TokenKind.Eof`"**，与具体诊断码无关。

**决定：**
- 标志位挂 `DiagnosticBag.IncompleteAtEof`（bool，默认 false）。**仅当此前无真语法错时**（`!HasErrors()`，即首个错误就发生在 EOF）才置位——否则「真错解析恢复后才走到 EOF」会被误判为不完整（如 `class 1` 已报 `expected type name`，就不能因后续 `_expect('{')` 走到 EOF 而当续读挂起）。
- 置位点（都判 `_peek().Kind == Eof`）：
  1. **主汇点** `Parser._expect` else 分支 —— 覆盖缺 `{` `}` `)` `]` 的绝大多数场景（`;` 单独，见下 Decision 3.1）。
  2. `ExprParser` 缺操作数 fallthrough（ExprParser.z42:294）—— 覆盖 `1 +`。
  3. `DeclParser` 名字位置内联报错（`expected type name` / `expected * name`）—— 覆盖 `class`＋EOF 等更早缺失。
- 诊断码：在这些 EOF 分支改报 `E0203 UnexpectedEof`（已定义未用），语义更准；非 EOF 仍报原码。`IncompleteAtEof` 标志位是 REPL 实际读取的信号，`E0203` 是配套的可读码。

### Decision 3.1: 缺 `;` 用**第二个**标志 `IncompleteSemiAtEof`（否则 `42` 被误判续读）

**坑（实测回归）：** `ParseStatement` 对表达式语句要求 `;` 结尾。REPL 表达式 `42` 无分号 → `_expectSemi` 缺 `;` 于 EOF → 若也置 `IncompleteAtEof`，`42` 会被判「没写完」→ 无限续读吃后续输入、**从不求值**。

**但** `void foo()`（声明入口，缺 body）也走 `_expectSemi` 缺 `;`——它**要**续读补 `{ }`。同样是「缺 `;` 于 EOF」，声明入口要续读、表达式入口不要。

**决定：** 缺 `;` 于 EOF 置**另一个**标志 `DiagnosticBag.IncompleteSemiAtEof`（`_expectSemi` 独立处理，不走 `_expect`），与 `IncompleteAtEof` 分开。`Completeness` 按入口取舍：

| 入口 | Classifier | 续读 = |
|------|-----------|--------|
| 声明 | `IsDecl` → `ParseCompilationUnit` | `IncompleteAtEof \|\| IncompleteSemiAtEof`（`void foo()` 缺 `;` 也续读） |
| 表达式/语句 | else → `ParseStatement` | 仅 `IncompleteAtEof`（`42` 缺 `;` **不**续读） |

### Decision 4: 完整性 = 语言真相（parser）；缩进 = 视觉装饰（native 括号法）

**问题：** 保留续行自动缩进（用户要求），缩进值谁算？若脚本层重写一套括号计数，又成 native/z42 双份。

**决定：** 职责分层——
- **完整性判定**是语言真相，必须 parser（脚本层 `Completeness`）。
- **续行缩进**是纯视觉装饰（`continuation_indent` 注释已言明：过/欠缩进不改语义），**留在 native 用括号法**完全可接受。

因此 native 保留 `bracket_depth` + `continuation_indent`，只服务缩进，**不再参与完整性判定**。新增 binding `Repl.ReadLineIndented(prompt, buf)`：native 内部 `continuation_indent(buf)` 算缩进 → `read_one_line(prompt, indent)`。脚本层续行时把已累积 buf 传进去。

### Decision 5: Ctrl-C 放弃缓冲 / Ctrl-D 退出（逃生机制）

**问题：** 悬挂续读（`1 +` 等下一行）后若打错了，用户要能放弃。所有主流 REPL 靠 Ctrl-C。

**决定：** 采用主流约定——续行途中 Ctrl-C（rustyline `Interrupted`）丢弃当前 buf、回主提示符；主提示符处 Ctrl-D（`Eof`）退出。**待实施时验证** `read_one_line` 当前是否把两者都归一为 `null`（Open Question）；若是，需让 native 返回可区分信号（如空串 sentinel 表示 Ctrl-C、`null` 表示 Ctrl-D，或新增返回码）。

### Decision 6: 悬挂表达式续读是 z42 语法特性的自然结果

**问题：** `1 +` 单独回车应续读还是报错？

**分析（其他语言分两派，分界是"换行是否是 token"）：** Node/C#/Ruby/Scala（换行不敏感）→ `1 +` 续读；Python（换行是 NEWLINE token）→ `1 +` 报 `invalid syntax`。

**决定：** z42 与 C#/JS 同属"换行不敏感、分号结尾"的 C 系，`1 + \n 2` 语法本就合法 → `1 +` **应续读**。这不是 REPL 的额外选择，而是 parser 权威判定的自然产物——选 B 的直接好处：REPL 续读行为 = 语法真相，永远自动一致，无需单独维护规则。

### Decision 7: 删除 `__repl_readblock` / `Repl.ReadBlock`（不留兼容）

多行累积上移脚本层后，native `__repl_readblock` 与 `Repl.ReadBlock` 无调用点。按 philosophy「不为旧版本提供兼容」直接删，不留兼容路径。删前 `grep` 确认无其它调用点。

## Implementation Notes

**`Completeness.IsIncomplete`（脚本层，裸 parse）：**
```
public static bool IsIncomplete(string src) {
    ParsedInput p = Classifier.Classify(src.Trim());
    Parser parser = new Parser(src, "<repl>");
    if (p.IsDecl) { parser.ParseCompilationUnit(); }   // class/fn 声明
    else          { parser.ParseStatement(); }          // 表达式/语句
    return parser.Diags.IncompleteAtEof;                // parser 需暴露 Diags 读取
}
```
- `Parser` 需暴露 `Diags`（现 `_diags` 为 public 字段，或加 `public bool IncompleteAtEof()` 便捷方法委托 `_diags`）。
- `ParseStatement` 只 parse 一个语句：REPL 一次一个 top item，足够。多语句输入判首个 item 的完整性（已知边界，罕见）。

**主循环（interactive_main，替换现 while 体）：**
```
string buf = "";
while (true) {
    Completer.SetActive(s);
    string prompt = (buf == "") ? ">>> " : "... ";
    string line = (buf == "") ? Repl.ReadLine(prompt)
                              : Repl.ReadLineIndented(prompt, buf);
    if (line == null) {                 // Ctrl-D
        if (buf != "") { buf = ""; continue; }   // buf 非空时的 EOF 语义 → 见 Decision 5
        Console.WriteLine(""); break;
    }
    // Ctrl-C（Interrupted sentinel）→ 丢弃 buf、continue（信号形式待 Decision 5 定）
    buf = (buf == "") ? line : buf + "\n" + line;
    string t = buf.Trim();
    if (buf == line && t.StartsWith(".")) { /* 元指令分派（仅首行、单行）*/ ...; buf=""; continue; }
    if (t.Length == 0) { buf = ""; continue; }
    if (Completeness.IsIncomplete(buf)) { continue; }   // 续读
    EvalResult r = Script.Eval(s, buf);
    if (!r.Success) { Console.Write(r.Error); }
    else if (r.HasValue && r.Value != null) { Console.WriteLine(_fmt(r.Value)); }
    buf = "";
}
```

**parser 置位（示意 `_expect`）：**
```
private void _expect(int kind, string what) {
    if (this._peek().Kind == kind) { this._advance(); return; }
    if (this._peek().Kind == TokenKind.Eof) {
        this._diags.MarkIncompleteAtEof();
        this._diags.Error(DiagnosticCodes.UnexpectedEof, "unexpected end of input, expected '" + what + "'", this._peek().Span);
    } else {
        this._diags.Error(DiagnosticCodes.ExpectedToken, "expected '" + what + "'", this._peek().Span);
    }
}
```

**native `__repl_readline_indented`（repl.rs）：**
```
// 参数：prompt, buf；内部 indent = continuation_indent(buf)，读一行预填 indent
let indent = continuation_indent(&buf);
read_one_line(ctx, &prompt, &indent)
```

## Testing Strategy

- **parser 单元测试**（`z42c.syntax/tests/incomplete-at-eof/`）：`class B` / `class B {` / `void foo()` / `1 +` / `f(1,` / `if(x){` → `IncompleteAtEof==true`；`class B {}` / `1 + 2` / `void foo();` / `class 1`（真错）→ `false`。
- **`Completeness` 单元测试**（`scripting/tests/completeness/`）：分派正确（声明走 CU、表达式走 Stmt）、各完整/不完整用例、不改 `ScriptState`。
- **Rust 单测**（`repl_tests.rs`，`cargo test --lib`）：`continuation_indent` 缩进保持；新 `readline_indented` 路径（缩进由 buf 括号深度决定）；删除的 `bracket_depth`-作完整性判定语义相应清理。
- **REPL 端到端手验**：`class B` ↦ 续行 ↦ 补全类体 ↦ 求值；`1 +` ↦ 续行；Ctrl-C 放弃；Ctrl-D 退出。
- **完整 GREEN**：`xtask test`（含 e2e / stdlib / compiler / vscode-syntax）。注意 [green-gate-skips-scripting-interactive]：`z42.scripting`/`z42.interactive` 不在默认 [Test] gate 内，改后须显式 `xtask build toolchain` 并把新鲜 `z42.scripting.zpkg`/`z42.interactive` 拷进验收环境手验交互。

## Deferred / Future Work

### repl-completeness-future-generic-return: 泛型返回类型函数头的续读

- **来源**：add-repl-parser-completeness 实施
- **触发原因**：`Classifier` 保守，`List<int> foo()` 因 `<>` 非括号被漏判为表达式 → 走 `ParseStatement`（表达式入口）而非声明入口 → 缺 body 时报错而非续读。
- **前置依赖**：`Classifier` 泛型感知，或转 parser 权威 submission 模式（本 change 未采纳的 `ParseSubmission`，见 §3 决策记录）。
- **触发条件**：REPL 用户频繁定义泛型返回类型的自由函数、报怨无法多行续读。
- **当前 workaround**：单行写完 `List<int> foo() { ... }`；或返回类型改用具体类型 / `var`（若语法允许）。
