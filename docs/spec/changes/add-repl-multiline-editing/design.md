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

> 保留 `ReplHelper::Validator` 实现为兜底：即便自定义 Enter handler 未注册（非交互/降级），一个最小
> `validate`（重入 `IsIncomplete` 或保守返回 `Valid`）能让 rustyline 的默认 `AcceptOrInsertLine`
> 仍工作。二者不冲突：handler 命中时走 handler，未命中回落 Validator/默认。**具体是否需要 Validator
> 兜底、还是 handler 足矣，实施期 spike 定。**

### Decision 2: Enter 位于缓冲中段的语义

**问题：** 光标不在缓冲末尾时按 Enter（用户在回改中间某行）——提交还是插行？

**决定：** 仅当**光标在缓冲末尾 且 整块完整**才提交；否则插换行（拆行编辑）。等价于 rustyline
`AcceptOrInsertLine { accept_in_the_middle: false }` 的直觉。z42 回车策略据 `(line, pos)` 判断：
`pos == line.Length && !IsIncomplete(line)` → accept，否则 newline。
（`line` 此处是**整块**含 `\n`；`pos` 是全局光标偏移。）

### Decision 3: 脚本层循环塌缩 + 删旧机制（无兼容）

`interactive_main.z42` 主循环从「per-line 累积」塌缩为「一次 ReadLine 拿整条语句 → 求值」：

```
loop:
  Completer.SetActive(s)
  string stmt = Repl.ReadLine(">>> ")   // 整条多行语句，一次返回；null=EOF/中断
  if stmt == null: break
  string t = stmt.Trim()
  if t.Length == 0: continue
  if t.StartsWith("."): <元指令分派>; continue     // 元指令天然单行完整（见 Decision 4）
  EvalResult r = Script.Eval(s, stmt)
  <打印>
```

**删除**（pre-1.0 无兼容，[[philosophy]] 不留旧路径）：`buf` 累积、`while` 续读、`... ` 续行提示符在
脚本层的驱动、`Completeness.ContinuationIndent` 经 `initial` 预填的调用点。`ContinuationIndent` 本身
**保留**（移到回车策略里调用）。`Repl.ReadLine` 的 `initial` 参数：整块模型下续行缩进由回车 handler
在缓冲内插入，不再需要 `initial` 预填 → `ReadLine` 签名简化为单参 `ReadLine(prompt)`（连带 Rust
`__repl_readline` + `read_one_line` 去掉 `initial`）。

> `... ` 续行提示符：整块模型下一次 readline 内部跨行，rustyline 多行渲染默认续行不换提示符（或用
> 空续行提示）。是否要视觉区分首/续行提示符，实施期按 rustyline 多行渲染能力定；不影响语义。

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
