# z42.scripting

## 职责
REPL / 脚本场景的**编译+执行层**（scripting-charter Form B）：把一段 z42 源即时编译成
内存 zpkg、加载进 live VM、反射调用求值。是 `z42.interactive`(z42i) 的引擎，用户代码也可 import。

## 功能索引
| 功能 | 入口 / 文件 |
|------|-----------|
| 行编辑（rustyline）| `Std.Repl.Repl.ReadLine/ReadBlock`（`Repl.z42` → z42vm builtin）|
| 内存加载编译产物 | `Std.Scripting.Engine.LoadBytes`（`__load_bytecode_in_memory`）|
| 按 FQN 调自由函数取结果 | `Std.Scripting.Engine.Invoke`（`__invoke_static`）|
| 会话状态 / 结果 | `ScriptState.z42`（含 `DeclNames`/`DeclTypeNames`/`DeclNamespaces`）/ `EvalResult.z42` |
| 输入分类（using/var/顶层声明/表达式/语句；类型 vs 自由函数）| `Classifier.z42`（`Classify` + `ParsedInput.IsTypeDecl`）|
| 编译+执行编排 | `Script.z42`（`Create` / `Eval`）|
| 函数/类型声明累积（跨轮）| `Script._evalDecl`——声明入 `Repl.R{N}` ns，`ExtendWithPackage`+`using` 供后续轮解析；重定义 ERROR；类型名并记 `DeclTypeNames`（`.types`）|
| 格式版本（`.version` 数据源）| `Script.FormatVersion`——zbc/zpkg strict-pin 版本串 |
| 多行输入 | `Std.Repl.ReadBlock`（括号平衡；宿主 `interactive_main` 接线）|

## 基础用法
```z42
ScriptState s = Script.Create();
EvalResult r = Script.Eval(s, "1 + 2");            // 表达式 → r.Value = 3
Script.Eval(s, "int add(int a, int b){ return a+b; }");  // 声明累积
EvalResult r2 = Script.Eval(s, "add(4, 5)");       // 跨轮裸调 → r2.Value = 9
```

## 如何测试验证
依赖编译器包（compiler-consuming 库），用「warm z42c + z42vm」回路编译：
```bash
# 组装 Z42_LIBS = 编译器 dist + stdlib dist（真实拷贝，非 symlink——lazy loader 不跟随 symlink）
z42vm z42c.driver.zpkg --mode interp -- build src/toolchain/scripting/z42.scripting.z42.toml --release --output-dir <out>
```
CI 全量 GREEN 以 toolchain 构建（`xtask build toolchain`）为准。

> **位置（D2 层级）**：本库虽以 `import z42.scripting` 被引用，物理上**不在** `src/libraries/`
> 而在 `src/toolchain/scripting/`。原因：`src/libraries/z42.workspace.toml` 的 `members=["*"]`
> 会把每个子目录当基 stdlib 成员，用「Z42_LIBS 仅含 stdlib」的路径 `build --workspace`；而本库依赖
> `z42c.*` 编译器包，在那条路径下会 `E0401: undefined Lexer/TokenKind` 而炸。移出 glob 后，由
> `xtask_toolchain.z42` 的 `_buildScriptingLib` 用「stdlib + z42c 合并 Z42_LIBS」专门构建。

## 关联文档
- 设计/机制：[`docs/design/toolchain/repl.md`](../../../docs/design/toolchain/repl.md)
- 引入/演进：change `add-z42-repl`（`docs/spec/changes/`；D2 依赖层级 / D7 命名 / D8 状态模型）

## 核心文件
| 文件 | 职责 |
|------|------|
| `Repl.z42` | `Std.Repl.Repl` 行编辑原生绑定（`ReadLine` / `ReadBlock`）|
| `Engine.z42` | `Std.Scripting.Engine` 内存加载 + FQN 调用原语 |
| `ScriptState.z42` / `EvalResult.z42` | 会话状态（含声明累积表）/ eval 结果 |
| `Classifier.z42` | 输入分类：using / var / 顶层函数·类型声明 / 表达式·语句 |
| `Rewriter.z42` | 会话变量裸引用 → `Vars{N}.x` 限定改写 |
| `Script.z42` | `Script.Create` / `Eval`（分类→建源→编译→加载→求值；声明累积）|
