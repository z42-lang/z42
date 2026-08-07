# Tasks: REPL parser 权威的输入完整性判定

> 状态：🟢 已完成 | 创建：2026-08-07 | 完成：2026-08-08

## 进度概览
- [ ] 阶段 1: parser 完整性信号（`IncompleteAtEof`）
- [ ] 阶段 2: 脚本层 `Completeness` 探针
- [ ] 阶段 3: native 单行读取 + 缩进 binding
- [ ] 阶段 4: REPL 主循环改造 + 逃生机制
- [ ] 阶段 5: 测试
- [ ] 阶段 6: 文档同步与验证

## 阶段 1: parser 完整性信号（按 pipeline：Parser）
- [ ] 1.1 `DiagnosticBag.z42` 加 `bool IncompleteAtEof`（默认 false）+ `MarkIncompleteAtEof()`；只读暴露
- [ ] 1.2 `Parser.z42` `_expect` else 分支：`_peek()==Eof` 时 `MarkIncompleteAtEof` + 报 `E0203 UnexpectedEof`（非 EOF 仍报 `E0202`）
- [ ] 1.3 `Parser` 暴露 `Diags`（确认 `_diags` public 可读，或加便捷 getter）
- [ ] 1.4 `ExprParser.z42` 缺操作数 fallthrough（:294）：`_peek()==Eof` 时置位（覆盖 `1 +`）
- [ ] 1.5 `DeclParser.z42` 名字位置内联报错：`_peek()==Eof` 时置位（覆盖 `class`＋EOF）
- [ ] 1.6 确认 `void foo()`（`_expectSemi` 走 `_expect`）自动被 1.2 覆盖

## 阶段 2: 脚本层 Completeness 探针
- [ ] 2.1 新建 `src/toolchain/scripting/src/Completeness.z42`：`IsIncomplete(string src) -> bool`
- [ ] 2.2 用 `Classifier.Classify` 分派：`IsDecl` → `ParseCompilationUnit`；else → `ParseStatement`；读 `Diags.IncompleteAtEof`
- [ ] 2.3 确认只 parse、不执行、不改 `ScriptState`

## 阶段 3: native 单行读取 + 缩进 binding
- [ ] 3.1 `repl.rs` 加 `builtin_repl_readline_indented(prompt, buf)`：`continuation_indent(buf)` → `read_one_line`
- [ ] 3.2 `mod.rs` 注册 `__repl_readline_indented`
- [ ] 3.3 `Repl.z42` 加 `ReadLineIndented(prompt, buf)` binding
- [ ] 3.4 删 `__repl_readblock`（repl.rs）+ `mod.rs` 注销 + `Repl.z42` 删 `ReadBlock`；`grep` 确认无其它调用点
- [ ] 3.5 保留 `bracket_depth`/`continuation_indent`（仅供缩进；不再作完整性判定）
- [ ] 3.6 Ctrl-C（Interrupted）vs Ctrl-D（Eof）区分：核对 `read_one_line` 现行为，按 design Decision 5 落地可区分信号

## 阶段 4: REPL 主循环改造
- [ ] 4.1 `interactive_main.z42` 主循环：`buf` 累积；首行 `ReadLine(">>> ")`、续行 `ReadLineIndented("... ", buf)`
- [ ] 4.2 `Completeness.IsIncomplete(buf)` 为真 → 续读；为假 → `Script.Eval` 求值后清 buf
- [ ] 4.3 元指令（`.`）仅首行单行分派；Ctrl-C 丢弃 buf 回主提示符；Ctrl-D 退出
- [ ] 4.4 `-c` 单次路径：`IsIncomplete` 为真时当语法错误、非零退出

## 阶段 5: 测试
- [ ] 5.1 `z42c.syntax/tests/incomplete-at-eof/`：class/fn/表达式/括号/完整不误判/真错不误判
- [ ] 5.2 `scripting/tests/completeness/`：分派 + 完整性各用例 + 不改 state
- [ ] 5.3 `repl_tests.rs`（`cargo test --lib`）：`continuation_indent` 保持 + `readline_indented` 缩进
- [ ] 5.4 REPL e2e 手验：`class B` 续行→补体→求值；`1 +` 续行；Ctrl-C 放弃；Ctrl-D 退出

## 阶段 6: 文档同步与验证
- [ ] 6.1 `docs/book/src/toolchain/repl.md`：完整性判定机制（parser 权威 + 探针解耦 + 缩进装饰 + mermaid）
- [ ] 6.2 `docs/book/src/compiler/diagnostics.md`：`E0203`/`IncompleteAtEof` 可恢复不完整语义（无页则新写 + 挂 SUMMARY）
- [ ] 6.3 `src/toolchain/scripting/README.md` + `interactive/README.md`：功能索引/核心文件/续行机制更新
- [ ] 6.4 `cargo build --release`（z42vm）无错
- [ ] 6.5 `xtask build toolchain` + `xtask test`（e2e/stdlib/compiler/vscode-syntax）全绿
- [ ] 6.6 自举不动点确认（改了 parser：gen1==gen2 字节一致）
- [ ] 6.7 spec scenarios 逐条覆盖确认
- [ ] 6.8 Deferred 登记：泛型返回类型函数头续行（design/roadmap Deferred Backlog Index）

## 备注
- 关键约束：完整性判定必须裸 parse（不包裹），否则包裹尾部 token 吞掉 EOF 缺失点（design Decision 1）。
- 探针解耦 → `EvalResult`/`CompiledModuleZ`/`CompileArtifacts` 均不改（design Decision 2）。
- 改了 parser 诊断 → 触发自举一代重建（gen1≠gen2），warm 重建自愈；6.6 确认不动点。
