# Design: REPL 整块多行编辑

## Architecture

```
旧（逐行）:
  interactive_main.z42                      repl.rs
  ┌──────────────────────────┐             ┌──────────────────────┐
  │ buf = ""                  │             │ read_one_line:       │
  │ loop:                     │  ReadLine   │   ed.readline()       │  ← 一次读 1 物理行
  │   line = ReadLine(p, ind) │────────────▶│   (initial 预填缩进)  │
  │   buf += line             │◀────────────│   return String      │
  │   if IsIncomplete(buf):   │  1 物理行    └──────────────────────┘
  │       continue            │
  │   Eval(buf); buf=""       │   ← 累积/完整性判定在脚本层
  └──────────────────────────┘

新（整块）:
  interactive_main.z42                      repl.rs + repl_editing.rs
  ┌──────────────────────────┐             ┌───────────────────────────────┐
  │ loop:                     │  ReadLine   │ read_one_line:                │
  │   stmt = ReadLine(">>> ") │────────────▶│   ed.readline()  ← 整块缓冲    │
  │   (整条多行语句)          │◀────────────│     每按 Enter → EnterHandler: │
  │   Eval(stmt)              │  整条语句    │       重入 z42 回车策略        │
  └──────────────────────────┘             │       accept→AcceptLine        │
                                            │       newline:<ind>→Insert(\n+ind)│
                                            └───────────────────────────────┘
                                                        │ 重入（ACTIVE_CTX）
                                                        ▼
                                            ReplEditing.z42 回车策略:
                                              IsIncomplete(整块)?
                                                false → "accept"
                                                true  → "newline:" + ContinuationIndent(整块)
```

完整性判定与续行缩进**从脚本层循环下沉到回车键回调**，但**判定算法本身不变**（仍是
`Completeness.IsIncomplete` / `ContinuationIndent`，parser 权威）。Rust 只多绑一个 Enter 键、多译两个
动作串——完全复用 `replComplete`（Tab 补全）/ `replKeyEdit`（缩进键）的重入范式。

## Decisions

### Decision 1: 回车机制——自定义 Enter handler（z42 权威）而非纯 rustyline Validator

**问题：** 整块多行需要「Enter 时判完整性 → 提交 or 插换行」。两条路：

**选项 A — 纯 rustyline `Validator`**：实现 `ReplHelper::Validator::validate()` 返回
`ValidationResult::Incomplete`（未写完）/ `Valid(None)`（写完）。rustyline 默认 Enter 绑定
`AcceptOrInsertLine` 会调 `validate`，Incomplete 时自插换行。
- ✅ 最少代码，idiomatic。
- ❌ **rustyline 自插的换行不带自动缩进** → 多行块全部顶格，编辑体验倒退（当前逐行模型是有续行缩进的）。
- ❌ 判定逻辑要么在 Rust（`validate` 里）重写、要么 `validate` 也重入 z42——但 `validate` 的重入时机
  （校验期）与 mutator 上下文的交互需额外验证。

**选项 B — 自定义 Enter handler，动作串协议（推荐）**：绑定 `KeyCode::Enter` 到一个
`ConditionalEventHandler`，按下时重入 z42 回车策略，z42 返回：
- `"accept"` → `Cmd::AcceptLine`（提交整块）
- `"newline:<indent>"` → `Cmd::Insert(1, "\n" + indent)`（插换行 + 缩进，光标落其后）
- ✅ **续行缩进可控**（复用 `ContinuationIndent`），编辑体验不倒退。
- ✅ 与现有 `replKeyEdit`（Backspace/Tab）**同一动作串范式**——Rust 仍是「dumb translator」，逻辑全在 z42。
- ✅ `Cmd::Insert(1, text)` 已验证 redo-免疫 + 光标正确（见 add-repl-tab-grid-snap），此处直接复用。
- ⚠️ 需处理 Enter 位于缓冲中段（见 Decision 2）与 rustyline 对 `AcceptLine` 的行为。

**决定：选 B。** 理由：缩进控制是多行编辑体验的核心，选项 A 丢缩进不可接受；且 B 与既有 keyedit
范式一脉相承（Rust dumb、逻辑在 z42），维护心智一致。**代价**：Enter 中段语义需显式设计（Decision 2）。

> **spike 已坐实（rustyline 14.0.0 源码，2026-08-23）——无需 Validator，handler 独控：**
> - `command.rs:133-158`：`Cmd::AcceptLine` 命中 `(Cmd::AcceptLine, ..)` 分支 → **无条件 `Submit`**，
>   与 validator 结果 / 光标位置无关。故 handler 返回 `AcceptLine` 即提交，validator 被忽略。
> - `lib.rs:516-535`：`ValidationResult::Incomplete` 时 rustyline 只重加换行、**不缩进**（证实选项 A 丢缩进）。
> - `binding.rs:183/189`：`EventContext::line()` 返回**整块缓冲**（多行含 `\n`），`pos()` 是全局字节偏移。
> - 无 handler 命中（未注册/出错回 `None`）→ 回落默认 ENTER = `AcceptOrInsertLine{accept_in_the_middle:true}`，
>   无 validator 时 `s.validate()` 返回 `Valid`（edit.rs:231）→ 提交单行。**安全回落**。
>
> **结论：`ReplHelper::Validator` 保持空 stub，不需要兜底 validator**——自定义 Enter handler 完全接管。

### Decision 2: Enter 位于缓冲中段的语义

**问题：** 光标不在缓冲末尾时按 Enter（用户在回改中间某行）——提交还是插行？

**决定：** 仅当**光标在缓冲末尾 且 整块完整**才提交；否则插换行（拆行编辑）。等价于 rustyline
`AcceptOrInsertLine { accept_in_the_middle: false }` 的直觉。z42 回车策略据 `(line, pos)` 判断：
`pos == line.Length && !IsIncomplete(line)` → accept，否则 newline。
（`line` 此处是**整块**含 `\n`；`pos` 是全局光标偏移。）

### Decision 3: 脚本层循环——保留累积（非 tty 需要），只删 `initial` 预填【实施期修正】

> **DRAFT 原设想「塌缩循环」，实施期修正为「保留累积」**：原以为一次 ReadLine 拿整条语句即可、
> 循环可删累积。但**非交互（管道 / 无 tty）路径** `plain_readline` 只能读**一个物理行**（无行编辑器）
> → 若删累积，piped 多行输入（`echo "int f(){\n...}" | z42i`、测试）会拿到半条语句求值报错。故
> **循环保留 `buf` 累积 + `Completeness.IsIncomplete` 判定**，两模式通吃：
> - **tty**：回车 handler 在**一次 readline 内**插换行 + 缩进直到完整，返回整块 → `buf`=整块、
>   `IsIncomplete` 一轮即 false → 求值。整块编辑（跨行导航 / 粘贴回改）全发生在这一次 readline 内。
> - **非 tty**：ReadLine 读一物理行 → `buf` 逐行累积 → `IsIncomplete` 判续读（同 add-repl-parser-completeness）。

实际循环改动**极小**：只把 `Repl.ReadLine(prompt, initial)` 的 `initial`/`ContinuationIndent` 实参删掉
（续行缩进移进回车策略在缓冲内插入），其余累积结构不动：

```
if (buf.Length == 0) { line = Repl.ReadLine(">>> "); }
else                 { line = Repl.ReadLine("... "); }   // ... 仅非 tty 逐行时出现
```

**删除**（pre-1.0 无兼容）：`Repl.ReadLine` 的 `initial` 参数（连带 Rust `__repl_readline` /
`read_one_line` / `plain_readline` 去掉 `initial`）、脚本层对 `Completeness.ContinuationIndent` 的调用
（移到回车策略）。`ContinuationIndent` 本体保留。

### Decision 6: Ctrl-C（中断）vs Ctrl-D（EOF）在整块模型下必须分流【实施期新增】

**问题（PTY 实测暴露的回归）**：旧逐行模型里 Ctrl-C / Ctrl-D 都映射成 `null`，靠 `buf` 是否非空区分
「续读中中断（弃缓冲、回主提示符）」vs「主提示符 EOF（退出）」。整块模型下多行编辑发生在**一次
readline 内**、`buf` 始终空 → Ctrl-C 中断该 readline 时 `buf` 空 → 旧逻辑误判为 EOF → **整个 REPL 退出**（回归）。

**决定**：在 Rust `read_one_line` 层分流——
- `Err(Interrupted)`（Ctrl-C）→ 返回**空串** `""`：循环视作「没输入、continue」→ 丢弃 rustyline 内已弃的
  多行缓冲、回主提示符（Python 式：Ctrl-C 重来）。
- `Err(Eof)`（Ctrl-D）→ 返回 `null`：循环 break → 退出 REPL。

副作用（可接受的行为改进）：主提示符下 Ctrl-C 由「退出」变「重来一行」（与 Python REPL 一致；Ctrl-D 仍退出）。

### Decision 4: 元指令识别点

旧模型：元指令只在 `buf.Length == 0`（首行）判。新模型一次拿整条语句，元指令仍是**单行**
（`.help` 等本就完整，`IsIncomplete` 为 false → 一次 readline 即提交）→ 在拿到 `stmt` 后 `Trim` 判
`.` 前缀即可，无需在读取中途特判。**唯一风险**：用户在多行缓冲里首行打 `.help` 再回车会怎样——
`.help` 单行完整 → 首个 Enter 即 accept 提交 `".help"`，符合预期（元指令不进多行）。

### Decision 5: 为 Deferred（`}`/floor）铺地基，但本 change 不做

整块缓冲模型下，`}` auto-dedent 与退格 floor 的实现路径变干净：它们成为**当前物理行内**的编辑，
可用已验证的 `Cmd::Dedent(WholeLine)` / `Cmd::Insert` 组合，不再需要 `Replace(WholeLine)`（那个撞
`edit_insert_text` 光标坑）。**但本 change 只铺地基、不实现这两项**——它们仍是 Deferred，待整块模型
落地稳定后作独立细化 change。design 在此登记接法，roadmap Deferred 表更新依赖关系。

## Implementation Notes

- **重入范式**：Enter handler 完全照抄 `KeyEditHandler`（repl_editing.rs:135-173）：读
  `active_ctx_ptr()` → `NativeUnparkGuard::exit` → `key_edit_via_callback`（或新增 enter 专用回调）→
  `parse_action`。可能复用 `KeyEditHandler` + 扩 `parse_action` 认 `"accept"`/`"newline:"`，或新增
  `EnterEditHandler`。倾向复用（Enter 也走 `replKeyEdit("enter", line, pos)`，少一个 builtin）。
- **`parse_action` 扩展**（repl_editing.rs:123）：
  - `"accept"` → `Some(Cmd::AcceptLine)`
  - `_ if starts_with("newline:")` → `Some(Cmd::Insert(1, "\n".to_string() + rest))`
  - 现有 `"dedent"` / `"insert:"` / `""` 不变。
- **z42 回车策略**（ReplEditing.z42，扩 `KeyEdit` 或新函数）：
  ```
  if key == "enter":
      # line=整块含\n，pos=全局光标偏移
      if pos == line.Length && !Completeness.IsIncomplete(line): return "accept"
      return "newline:" + Completeness.ContinuationIndent(line[..pos] 或整块)  # 缩进基准待定
  ```
  ⚠️ `ContinuationIndent` 现按整个 buf 的未闭合层数算。回车插入点若在中段，缩进基准该用「光标前文本」
  还是「整块」——实施期定（编辑器直觉：按光标前的未闭合层数）。
- **Enter 键绑定**：`read_one_line` 里 `ed.bind_sequence(KeyEvent(KeyCode::Enter, NONE), Conditional(EnterHandler))`，
  与现有 Backspace/Tab 绑定并列。
- **`ReplEditing` 跨包 E0401 坑**（[[add-repl-indent-editing]] 供种教训）：golden 仍须**同包**编
  driver + 本地 `ReplEditing.z42` 副本（生产按 string FQN 引 `replKeyEdit`，从不 compile-time 引类）。
- **arity**：若 Enter 复用 `replKeyEdit(key,line,pos)` 3 参，`key_edit_arity_check` 不变；若新增 enter
  回调另定 arity。

## Testing Strategy

- **单元（Rust）**：`repl_editing_tests.rs` 加 `parse_action("accept")` → `AcceptLine`、
  `parse_action("newline:    ")` → `Insert(1, "\n    ")` 的映射断言。
- **Golden（z42 纯函数）**：`tests/repl_editing/driver.z42` 加回车策略用例——各种 `(line, pos)` →
  `accept` / `newline:<ind>`（覆盖：完整末尾、不完整、光标中段、多层缩进）。同包编译（E0401 坑）。
- **PTY e2e（交互）**：python `pty.fork` 拉起 z42vm + z42.interactive，喂键序列验证：
  ① `class C {` + Enter → 缩进续行不提交；② 补全 `}` + Enter → 提交；③ 粘贴多行块 → 方向键上移改行
  → 提交改后整块；④ 多行中 Ctrl-C → 回主提示符；⑤ `.help` + Enter → 立即元指令。
  （scripting/interactive 不进自动 GREEN gate，见 [[green-gate-skips-scripting-interactive]] → PTY + golden 兜。）
- **完整 GREEN**：`xtask test`（全 stage）。本 change 不改编译 pipeline，重点在 e2e / stdlib 不回归。

## Deferred / Future Work

### repl-multiline-future-rbrace-floor
- **来源**：本 change design Decision 5 + [[add-repl-indent-editing]] 归档 Deferred
- **触发原因**：整块模型先落地稳定，`}`/floor 作独立细化，避免一次改动过大。
- **前置依赖**：本 change（整块缓冲模型）合并。
- **触发条件**：整块模型稳定后，若用户要 `}` 自动回退一级 / 退格 floor 到前制表位。
- **当前 workaround**：手动退格；`}` 手动对齐。
