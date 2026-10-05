# z42.scripting

## 职责
REPL / 脚本场景的**跨平台编译+执行内核**（scripting-charter Form B）：把一段 z42 源即时编译成内存 zpkg、加载进 live VM、反射调用求值，并提供补全与输入完整性判定。是 `z42.interactive`（z42i）的引擎，playground / 用户代码也可 import。**不做**终端行编辑（tty）——那在 [z42.repl](../../toolchain/interactive/repl/)。

**编译期只依赖 stdlib**：编译经 `z42.build` 的 `IReplCompiler` 门面（把「有状态增量编译世界」封成 opaque 句柄），实现 `Z42cReplCompiler` 住 `z42c.pipeline`，由 `ReplCompilerHost` 运行期反射注入（apphost 不静态 bundle 编译器，运行期动态加载 `z42c.pipeline` 组件）。前端 `z42c.core` / `z42c.syntax`（Lexer/Parser/Span）为库依赖。机制详见 [repl.md「编译门面 + 运行期注入」](../../../docs/internals/src/toolchain/repl.md)。

## 功能索引
命名空间 `Std.Scripting`。

| 功能 | 入口 / 文件 |
|------|-----------|
| 内存加载编译产物 | `Engine.LoadBytes`（`__load_bytecode_in_memory`）|
| 按 FQN 调自由函数取结果 | `Engine.Invoke`（`__invoke_static`）|
| 会话变量 live 值成员名反射（补全用）| `Engine.MemberNames`（`__repl_member_names`）|
| 会话状态 / 结果 | `ScriptState.z42`（含 `DeclNames`/`DeclTypeNames`/`DeclNamespaces`）/ `EvalResult.z42` |
| 输入分类（using/var/顶层声明/表达式/语句；类型 vs 自由函数）| `Classifier.z42`（`Classify` + `ParsedInput.IsTypeDecl`；`_typeRefEnd` 跳完整类型引用——限定名/泛型/数组/可空，识别 `List<int> a = new()` 等多 token 类型声明）|
| 编译+执行编排 | `Script.z42`（`Create` / `Eval`；编译经 `IReplCompiler.CompileRound`）。**求值期错误恢复**：编译错误及运行异常（用户 `throw` / 除零 / 越界 / `__box_prim` 类型不符）均被捕获、作失败 `EvalResult` 返回、会话不推进，异常不逃逸终止 REPL；异常轮仍前进 `Counter`（本轮模块已加载进 VM，重用轮号会让旧抛出函数「粘住」）|
| 编译器组件运行期注入 | `ReplCompilerHost.Get()`：`ModuleLoader.Load` z42c.pipeline.zpkg → 反射 `Z42cReplCompiler` → `as IReplCompiler`；缺失兜底 `NoReplCompiler` |
| 启动预热（后台线程建依赖世界）| `Script.Prewarm`（REPL 启动 spawn worker 跑；`_ensureWarm` 首次 Eval 前 Join 汇合）+ `ScriptState.PrewarmThread`；GC-safe park 见 z42vm `corelib/repl.rs` + `gc/safepoint.rs` |
| 函数/类型声明累积（跨轮）| `Script._evalDecl`——声明入 `Repl.R{N}` ns，`ExtendWithPackage` + `using` 供后续轮解析；重定义报 ERROR；类型名记 `DeclTypeNames`（`.types`）；缺省未写可见性的类型声明自动补 `public`（`Classifier.HasVisibility`），避开每轮独立 package 下 internal 类的 `E0441`/`E0404` |
| 多行输入完整性判定 | `Completeness.IsIncomplete`——parser 权威：对**裸输入原文** parse，读 `IncompleteAtEof` 决定续读；tty 下由 `ReplEditing.KeyEdit` 的 Enter 分支对整块调用，非 tty 由 `interactive_main` 逐行累积调用 |
| 续行视觉缩进 | `Completeness.ContinuationIndent`——用 Lexer 数括号算 `层数×4 空格`；由 `ReplEditing.KeyEdit` 的 Enter 分支在缓冲内插换行时附加；纯装饰，不参与完整性判定 |
| Tab 补全 | `Completer.z42`（`replComplete`）|
| WASM playground 入口 | `Playground.z42` 的 `evalFromVfs`：读 VFS `/input.z42`，经 `Script.CreateWithLibs("/libs")` 求值，输出走 Console |

## 基础用法
```z42
ScriptState s = Script.Create();
EvalResult r = Script.Eval(s, "1 + 2");            // 表达式 → r.Value = 3
Script.Eval(s, "int add(int a, int b){ return a+b; }");  // 声明累积
EvalResult r2 = Script.Eval(s, "add(4, 5)");       // 跨轮裸调 → r2.Value = 9
```

## 如何测试验证
本库随编译器 workspace 构建，`tests/` 由 stdlib 跑测器枚举（`xtask test stdlib` 认 `src/compiler/` 与 `src/libraries/` 两个根）：
```bash
./xtask build compiler                 # 含本库；产物 artifacts/build/compiler/z42.scripting/release/dist/z42.scripting.zpkg
./xtask test stdlib z42.scripting      # 枚举本包 tests/
./xtask build toolchain                # 编 z42.repl / z42.interactive，验证下游可链接
```

## 关联文档
- 设计 / 机制：[repl.md](../../../docs/internals/src/toolchain/repl.md)（含输入完整性判定：parser 权威 / 探针解耦 / 裸 parse）
- 终端行编辑 / 键位（tier1）：[z42.repl](../../toolchain/interactive/repl/)

## 待办
- `tests/<case>/` 下 11 个 `driver.z42` + `expected_output.txt` 目录既无 `source.z42` 也无 `*.z42.toml`，尚未接成 `[Test]` 单元；跑测器每轮对其报孤儿源提示

## 核心文件
| 文件 | 职责 |
|------|------|
| `Completeness.z42` | 输入完整性探针 `IsIncomplete`（裸 parse 原文，读 parser `IncompleteAtEof`；与求值解耦）+ 续行缩进 `ContinuationIndent` |
| `Engine.z42` | 内存加载 + FQN 调用 + 成员名反射（`LoadBytes` / `Invoke` / `MemberNames`）|
| `Completer.z42` | Tab 补全：会话变量 / 声明名 / 导入世界 / live 值实例成员 |
| `ScriptState.z42` / `EvalResult.z42` | 会话状态（含声明累积表）/ eval 结果 |
| `Classifier.z42` | 输入分类：using / var / 顶层函数·类型声明 / 表达式·语句 |
| `Rewriter.z42` | 会话变量裸引用 → `Vars{N}.x` 限定改写 |
| `Script.z42` | `Script.Create` / `Eval`（分类→建源→编译→加载→求值；声明累积）+ `Prewarm` / `_ensureWarm` |
| `ReplCompilerHost.z42` | 运行期反射注入 `IReplCompiler` 实现 |
| `Playground.z42` | WASM playground 入口 `evalFromVfs` |

## 依赖关系
`z42.core`（String / Reflection）、`z42.io`（Environment / File / Path / Console，组件定位 + Z42_LIBS）、`z42.build`（`IReplCompiler` 门面）、`z42.test`（`ModuleLoader`）、`z42c.core` / `z42c.syntax`（Span / Lexer / Parser）、`z42.threading`（预热 worker）。编译器后端不静态依赖，运行期经反射注入。
