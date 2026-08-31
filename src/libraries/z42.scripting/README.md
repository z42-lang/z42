# z42.scripting

## 职责
REPL / 脚本场景的**跨平台编译+执行内核**（scripting-charter Form B）：把一段 z42 源即时编译成
内存 zpkg、加载进 live VM、反射调用求值 + 补全 / 完整性判定。是 `z42.interactive`(z42i) 的引擎，
playground / 用户代码也可 import。**终端行编辑（tty，tier1）已拆到 [z42.repl](../repl/)**
（`split-z42-repl`），本库只保留跨平台 eval-core。

**编译期 stdlib-only（sink-repl-compile-facade）**：编译不再静态依赖编译器后端（`z42c.semantics`/
`z42c.pipeline`），改经 `z42.build` 的 `IReplCompiler` 门面——把「有状态增量编译世界」封成 opaque 句柄，
实现（`Z42cReplCompiler`，住 `z42c.pipeline`）由 `ReplCompilerHost` 运行期反射注入（mirror z42b
`_hostCompiler`）。前端 `z42c.core`/`z42c.syntax`（Lexer/Parser/Span）已是 stdlib（PR-A）。→ apphost
不再静态 bundle 编译器，运行期动态加载 `z42c.pipeline` 组件。机制详见
[repl.md「编译门面 + 运行期注入」](../../../docs/design/toolchain/repl.md)。（物理仍在 `src/toolchain`，
搬 `src/libraries` 作 follow-up。）

## 功能索引
| 功能 | 入口 / 文件 |
|------|-----------|
| 内存加载编译产物 | `Std.Scripting.Engine.LoadBytes`（`__load_bytecode_in_memory`）|
| 按 FQN 调自由函数取结果 | `Std.Scripting.Engine.Invoke`（`__invoke_static`）|
| 会话变量 live 值成员名反射（补全用）| `Std.Scripting.Engine.MemberNames`（`__repl_member_names`；反射查询，split-z42-repl 从 Std.Repl.Repl 下移消环）|
| 会话状态 / 结果 | `ScriptState.z42`（含 `DeclNames`/`DeclTypeNames`/`DeclNamespaces`）/ `EvalResult.z42` |
| 输入分类（using/var/顶层声明/表达式/语句；类型 vs 自由函数）| `Classifier.z42`（`Classify` + `ParsedInput.IsTypeDecl`；`_typeRefEnd` 跳完整类型引用——限定名/泛型/数组/可空，识别 `List<int> a = new()` 等多 token 类型声明，fix-repl-generic-decl-classify）|
| 编译+执行编排 | `Script.z42`（`Create` / `Eval`；编译经 `IReplCompiler` 门面 `CompileRound`）。**求值期错误恢复**：编译错误**及运行异常**（用户 `throw` / 除零 / 越界 / `int x = "s"` 之类的 `__box_prim` 类型不符）均被捕获、作失败 `EvalResult` 返回、会话不推进 → 异常不逃逸终止 REPL（fix-repl-eval-exception）；异常轮仍前进 `Counter`（本轮模块已加载进 VM，重用轮号会让旧抛出函数"粘住"）|
| 编译器组件运行期注入 | `ReplCompilerHost.z42`（`Get()`：`ModuleLoader.Load` z42c.pipeline.zpkg → 反射 `Z42cReplCompiler` → `as IReplCompiler`；缺失兜底 `NoReplCompiler`。sink-repl-compile-facade）|
| 启动预热（后台线程建依赖世界）| `Script.Prewarm`（REPL 启动 spawn worker 跑；`_ensureWarm` 首次 Eval 前 Join 汇合）+ `ScriptState.PrewarmThread`；GC-safe park 见 z42vm `corelib/repl.rs`+`gc/safepoint.rs`（add-repl-prewarm）|
| 函数/类型声明累积（跨轮）| `Script._evalDecl`——声明入 `Repl.R{N}` ns，`ExtendWithPackage`+`using` 供后续轮解析；重定义 ERROR；类型名并记 `DeclTypeNames`（`.types`）；**缺省未写可见性的类型声明自动补 `public`**（`Classifier.HasVisibility` 判定），避开每轮独立 package 下 internal 类的 `E0441`/`E0404`（fix-repl-default-type-visibility）|
| 格式版本（`.version` 数据源）| `Script.FormatVersion`——zbc/zpkg strict-pin 版本串 |
| 多行输入完整性判定 | `Completeness.IsIncomplete`（`Completeness.z42`）——parser 权威：对**裸输入原文** parse，读 `IncompleteAtEof` 决定续读；tty 下由 `ReplEditing.KeyEdit` 的 Enter 分支对整块调用、非 tty 由 `interactive_main` 逐行累积调用（add-repl-parser-completeness / add-repl-multiline-editing）|
| 续行视觉缩进 | `Completeness.ContinuationIndent`（`Completeness.z42`——用既有 Lexer 数括号算 `层数×4 空格`；由 `ReplEditing.KeyEdit` 的 Enter 分支在缓冲内插换行时附加；纯装饰，不参与完整性判定。sink-repl-indent-to-script / add-repl-multiline-editing）|

## 基础用法
```z42
ScriptState s = Script.Create();
EvalResult r = Script.Eval(s, "1 + 2");            // 表达式 → r.Value = 3
Script.Eval(s, "int add(int a, int b){ return a+b; }");  // 声明累积
EvalResult r2 = Script.Eval(s, "add(4, 5)");       // 跨轮裸调 → r2.Value = 9
```

## 如何测试验证
本库是 **stdlib workspace 成员**（`src/libraries/`），随 `xtask build stdlib`
（`z42c build --workspace`）编入 flat dist：
```bash
cd src/libraries && z42c build --workspace --release
# 产物：artifacts/build/libraries/z42.scripting/release/dist/z42.scripting.zpkg
```
CI 全量 GREEN 以 stdlib 构建（`xtask build stdlib`）+ toolchain 构建（`xtask build toolchain`，编
`z42.repl` / `z42.interactive`）为准。

> **位置（move-scripting-to-libraries）**：本库物理落 `src/libraries/`，是普通 stdlib 成员。
> 编译期**只依赖 stdlib**——编译走 `z42.build` 的 `IReplCompiler` 门面（实现 `Z42cReplCompiler` 住
> `z42c.pipeline`，运行期经 `ModuleLoader` 反射注入，见 `ReplCompilerHost`）；前端 `z42c.core/syntax`
> 亦已下沉 stdlib（PR-A）。故不再触发早期「`members=["*"]` 把子目录当基 stdlib 成员、依赖 `z42c.*`
> 编译器包会 `E0401` 而炸」的问题，无需 `src/toolchain/` 的合并-libs 特例步。tty 交互层 `z42.repl`
> 因真 tty + native 行编辑 builtin（平台绑定重）留 `src/toolchain/`。

## 关联文档
- 设计/机制：[`docs/design/toolchain/repl.md`](../../../docs/design/toolchain/repl.md)；
  输入完整性判定机制（parser 权威 / 探针解耦 / 裸 parse）见 [`docs/book/src/toolchain/repl-input-completeness.md`](../../../docs/book/src/toolchain/repl-input-completeness.md)
- 引入/演进：change `add-z42-repl`（`docs/spec/changes/`；D2 依赖层级 / D7 命名 / D8 状态模型）；
  完整性判定改 parser 权威见 change `add-repl-parser-completeness`；终端交互层拆出见 change `split-z42-repl`；
  求值期运行异常捕获（REPL 不再因 `throw`/除零/类型不符而退出）见 change `fix-repl-eval-exception`
- 终端行编辑 / 键位（tier1）：[z42.repl](../repl/)

## 核心文件
| 文件 | 职责 |
|------|------|
| `Completeness.z42` | 输入完整性探针 `IsIncomplete`（裸 parse 原文，读 parser `IncompleteAtEof`；与求值解耦）+ 续行缩进 `ContinuationIndent`（Lexer 数括号）——由 z42.repl 的 `ReplEditing.KeyEdit` Enter 分支 / `interactive_main` 逐行累积调用 |
| `Engine.z42` | `Std.Scripting.Engine` 内存加载 + FQN 调用 + 成员名反射（`LoadBytes` / `Invoke` / `MemberNames`）|
| `Completer.z42` | Tab 补全（`replComplete`）：会话变量 / 声明名 / 导入世界 / live 值实例成员（`Engine.MemberNames`）|
| `ScriptState.z42` / `EvalResult.z42` | 会话状态（含声明累积表）/ eval 结果 |
| `Classifier.z42` | 输入分类：using / var / 顶层函数·类型声明 / 表达式·语句 |
| `Rewriter.z42` | 会话变量裸引用 → `Vars{N}.x` 限定改写 |
| `Script.z42` | `Script.Create` / `Eval`（分类→建源→编译→加载→求值；声明累积）+ `Prewarm`/`_ensureWarm`（启动后台预热依赖世界 + 首次 Eval 汇合）|
