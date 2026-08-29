# z42.repl

## 职责
REPL **终端交互层（tier1）**：rustyline 行编辑 + 缩进感知键位策略。只在真 tty / 终端 REPL 用；
求值内核（编译 / 加载 / 反射求值 / 补全 / 完整性判定）在 [z42.scripting](../../../libraries/z42.scripting/)（已下沉 stdlib）。
拆自 z42.scripting（`split-z42-repl`），以切干净「跨平台 eval-core」与「tier1 tty 交互」的边界。

## 功能索引
| 功能 | 入口 / 文件 |
|------|-----------|
| 行编辑（rustyline，读整条多行语句 / 非 tty 读一物理行） | `Std.Repl.Repl.ReadLine(prompt)`（`Repl.z42` → z42vm builtin `__repl_readline`）|
| 注册 Tab 补全回调 FQN | `Std.Repl.Repl.SetCompleter(fqn)`（回调 `Std.Scripting.replComplete`）|
| 注册缩进键位回调 FQN | `Std.Repl.Repl.SetKeyEditor(fqn)`（回调本包 `replKeyEdit`）|
| 缩进感知键位 + 回车整块判定 | `Std.Repl.ReplEditing.KeyEdit(key,line,pos)`（`ReplEditing.z42`；退格→`dedent`｜错位 `replace:` floor、Tab→`insert:<空格>` grid-snap、`}`→`replace:<缩进>}` 自动回退、Enter→`accept`（写完提交）｜`newline:<缩进>`（没写完续行））+ 自由函数 `replKeyEdit`；决策全在 z42，Rust `parse_action` 只译成 redo-免疫 `Cmd`（见 host-only cdylib `crates/z42-repl/src/editing.rs`）|

## 基础用法
```z42
using Std.Repl;
string line = Repl.ReadLine(">>> ");        // tty 下读整条多行语句；EOF → null
Repl.SetCompleter("Std.Scripting.replComplete");   // Tab 补全（Completer 在 z42.scripting）
Repl.SetKeyEditor("Std.Repl.replKeyEdit");         // 缩进感知键位
```
整块编辑的完整性判定（「写完没」）与续行缩进复用 `Std.Scripting.Completeness`（本包依赖 z42.scripting）。

## 如何测试验证
compiler-consuming 库（依赖 z42.scripting → z42c.*），用「warm z42c + z42vm」回路编译；随 z42.interactive
一同构建：
```bash
xtask build toolchain    # 建 z42.scripting → z42.repl → z42.interactive（合并 Z42_LIBS）
```
键位策略的纯函数（`parse_action`）覆盖在 cdylib Rust 单测 `crates/z42-repl/src/editing.rs`（`cargo test -p z42-repl`）；
z42 侧 golden `tests/repl_editing/` 直调 `ReplEditing.KeyEdit`（与 rustyline 回调同一策略）。
CI 全量 GREEN 以 toolchain 构建（`xtask build toolchain`）+ dist smoke（`z42 repl -c "1+2"`）为准。

## 关联文档
- 设计/机制：[`docs/design/toolchain/repl.md`](../../../../docs/design/toolchain/repl.md)；
  键位适配壳（policy-free、动作串范式）见 host-only cdylib `crates/z42-repl/src/editing.rs` 头注
- 引入/演进：change `add-z42-repl`（REPL MVP）/ `add-repl-indent-editing` / `add-repl-tab-grid-snap` /
  `add-repl-multiline-editing` / `add-repl-rbrace-floor`（`}` 自动回退 + 退格 floor，patch rustyline 光标）；
  `split-z42-repl`（本包从 z42.scripting 拆出）

## 核心文件
| 文件 | 职责 |
|------|------|
| `Repl.z42` | `Std.Repl.Repl`：rustyline 行编辑原生绑定（`ReadLine` / `SetCompleter` / `SetKeyEditor`）|
| `ReplEditing.z42` | `Std.Repl.ReplEditing.KeyEdit`：键位策略（退格 `dedent`｜错位 `replace:` floor / Tab `insert:` grid-snap / `}` `replace:` 自动回退 / Enter `accept`｜`newline:<缩进>` 整块多行判定，复用 `Completeness`）+ 自由函数 `replKeyEdit` 回调 |
