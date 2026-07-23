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
| 会话状态 / 结果 | `ScriptState.z42` / `EvalResult.z42` |
| 编译+执行编排 | `Script.z42`（`Create` 就绪；`Eval` 待 design D8 裁决）|

## 基础用法
```z42
ScriptState s = Script.Create();
// EvalResult r = Script.Eval(s, "1 + 2");   // 待 D8 落地
```

## 如何测试验证
依赖编译器包（compiler-consuming 库），用「warm z42c + z42vm」回路编译：
```bash
# 组装 Z42_LIBS = 编译器 dist + stdlib dist（真实拷贝，非 symlink——lazy loader 不跟随 symlink）
z42vm z42c.driver.zpkg --mode interp -- build src/libraries/z42.scripting/z42.scripting.z42.toml --release --output-dir <out>
```
CI 全量 GREEN 以 `xtask test stdlib` 为准。

## 关联文档
- 设计/机制：[`docs/design/toolchain/repl.md`](../../../docs/design/toolchain/repl.md)
- 引入/演进：change `add-z42-repl`（`docs/spec/changes/`；D2 依赖层级 / D7 命名 / D8 状态模型）

## 核心文件
| 文件 | 职责 |
|------|------|
| `Repl.z42` | `Std.Repl.Repl` 行编辑原生绑定 |
| `Engine.z42` | `Std.Scripting.Engine` 内存加载 + FQN 调用原语 |
| `ScriptState.z42` / `EvalResult.z42` | 会话状态 / eval 结果 |
| `Script.z42` | `Script.Create` / `Eval`（编排；Eval 待 D8）|
