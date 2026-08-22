# Design: REPL Tab 缩进网格吸附

## Architecture

沿用 #253 的三层「策略在 z42、Rust 只做 policy-free 适配壳」：

```
按键 (Backspace / Tab)
      │  rustyline readline() 事件循环 (Rust)
      ▼
KeyEditHandler (repl_editing.rs)                 ← policy-free 适配壳（不变）
      │  取 ctx.line()/ctx.pos() + 键名，经 ACTIVE_CTX 重入 VM
      ▼
z42  ReplEditing.KeyEdit(key, line, pos) -> string  ← 全部策略在此
      │  ""  /  dedent  /  insert:<text>
      ▼
parse_action(s) -> Option<Cmd>
      │  ""→None(默认) · dedent→Dedent(WholeLine) · insert:→Insert(1,text)
      ▼
rustyline 执行 Cmd
```

本 change 只改一处策略（Tab 分支）+ 一个协议动作（`insert:`），不加键绑定、不动重入路径。

## Decisions

### Decision 1：Tab 用 grid-snap-ceil（`Insert` 补 delta），不用 `Indent`

**问题：** #253 的 Tab 发 `indent` → `Cmd::Indent(WholeLine)`，恒加 `indent_size`(=4)。故 `col=2` 按 Tab
→ `col=6`（不对齐制表位）。网格吸附要 ceil 到下制表位（`col=2` → `col=4`）。

**决定：** Tab 改发 `insert:<(next_stop - col) 个空格>` → `Cmd::Insert(1, spaces)`，在光标处补足到下制表位
`next_stop = ((col/4)+1)*4`。`Insert(1, text)`：文本在 payload、`redo(Some(1))` 只重复一次故 redo-免疫；
`edit_insert` 推进光标到补入之后（**光标正确**）；不走 `edit_kill` 故**不污染 kill-ring**。删除 vestigial 的
`indent` 动作（无人再 emit）。

对齐情形（`col%4==0`，含 `col=0`）：`next_stop = col+4`，补满一级，与 #253 等价。错位情形（`col=2`）：补到
制表位（补 2），这是新行为。

### Decision 2：退格保持 #253 的 `dedent`（floor 属 Deferred）

**问题：** 网格吸附的对称项是「退格 floor 到前制表位」——深错位（`col>4` 且非 4 倍数，如 6）应删到 4。
`Dedent(WholeLine)` 删 `indent_size`(=4)：`col=6` → `col=2`（越过制表位）。要 floor 到 4 需删变量个（2）。

**决定：** 维持 #253 `dedent`，**不做退格 floor**。唯一 redo-免疫的整行替换 `Replace(WholeLine, text)` 见
Decision 3 的光标问题；而 `Dedent` 对对齐/浅缩进已正确（`col%4==0` 删 4 到制表位；`col<4` 删到 0），只有
深错位（罕见——自动缩进恒产出 4 倍数）才不 floor。收益低、代价（光标）高，故 Deferred。

### Decision 3（关键，为何 `}` / 退格 floor 延后）：`Replace(WholeLine)` redo-免疫但光标归位行首

**问题：** #253 认定 `}` 回退 / 变量宽度删除「须 patch rustyline」，理由是 `Kill(BackwardChar(n))` 的计数被
`redo(Some(1))` 覆盖退化成删 1。

**调查更正：** `Cmd::Replace(Movement::WholeLine, Some(text))` **对 redo 免疫**——`WholeLine` 无 `RepeatCount`、
`redo` 恒等。z42 算好整行、Rust 整行替换，即可做 `}` 回退 / 退格 floor，redo-免疫**无需 fork**。PTY spike
实测 `Replace(WholeLine)`：退格深错位 6→4 ✓、`}` 全空白 8→`    }` ✓、`}` self-insert 兜底 ✓、kill-ring 污染
= 纯空格（可接受）✓。**#253「须 patch」的判据在 redo 维度被证伪。**

**但 spike 暴露另一处 rustyline 局限：** `Cmd::Replace` 执行 = `edit_kill(WholeLine)`（光标 move_home 到 col 0）
→ `edit_insert_text(text)`。而 `edit_insert_text`（line_buffer `insert_str`）插入后**不推进 `pos`** → 光标停在
**col 0**（行首）。实测：`}` → `    }` 后再打字得 `ab    }`（前置）；退格 6→4 后再打字得 `ab    `。**这破坏
`}` 之后继续输入（`} else {` / `};`）。**

**决定：** 因 z42 非缩进敏感，`}` 回退 / 退格 floor 纯属**视觉美化**，不值得为其接受功能性光标倒退，也不值得为
一个视觉特性 fork rustyline。二者维持 **Deferred**，理由从「须 patch rustyline（redo）」更正为「删+插的光标
正确需 patch `edit_insert_text` 使其推进光标」。本 change 只落地光标正确的 Tab-ceil。

### Decision 4：动作串格式 `insert:` 前缀 + 逐字余文

`parse_action` 对 `dedent` 精确匹配；`insert:` 用前缀剥离，冒号后整段逐字作文本（纯空格逐字保留）。文本内
不含冒号，无歧义。

## Implementation Notes

- **z42 `_spaces(int n)`**：n 个空格（n≥0）辅助。
- **`KeyEdit` Tab 分支**：`ws` → `next = ((col/4)+1)*4; return "insert:" + _spaces(next-col);`。退格/其它不变。
- **Rust `parse_action`**：`"dedent" => Dedent(WholeLine)`；`s.starts_with("insert:") => Insert(1, s[7..])`；余 `None`。
- 不加键绑定（复用 #253 的 Backspace/Tab 绑定）。

## Testing Strategy

- **z42 golden**（`tests/repl_editing/`）：Tab grid-snap ceil 全场景（`col` 0/1/2/4/6、有词、多行）；退格仍 dedent。
  手动跑通（scripting 不进自动 GREEN gate）：本地组 Z42_LIBS → 同包编 driver+ReplEditing → 建自 z42vm run → diff。
- **Rust 单测**（`repl_editing_tests.rs`）：`parse_action("insert:  ")` → `Insert(1,"  ")`；`"dedent"` → `Dedent`；
  `"indent"`/`"replace:.."`/未知 → `None`。
- **PTY spike（已跑）**：Tab `col=2` + Tab → 4 空格、光标停末尾 ✓。（`Replace` 光标归位行首亦已实测，据此定
  Deferred。）
- **GREEN**：`cargo build` + `cargo test --lib` + `xtask test` 全 stage（自举 5/5）。

## Deferred / Future Work

### repl-indent-future-rbrace-dedent: `}` 自动回退一级
- **来源**：#253 Deferred + 本 change spike。
- **触发原因（更正）**：可用 redo-免疫的 `Cmd::Replace(WholeLine, "<dedent 空格>}")` 实现（无需 fork），但
  rustyline `edit_insert_text` 插入后不推进光标 → 光标归位行首，破坏 `}` 之后继续输入（`} else {`）。
- **前置依赖**：patch rustyline `edit_insert_text` 使其推进 `pos`（或等价的删+插且光标落末尾的原语）；或走
  多行编辑重构（PR-B）后重新评估。
- **触发条件**：用户明确要求 `}` 自动对齐，且接受 rustyline fork；或 patch 落地。
- **当前 workaround**：`}` 起头的续行手动退格一次（#253 的 `dedent` 一次到位、光标正确）。

### repl-indent-future-grid-snap-backspace: 退格 floor 到前制表位
- **来源**：#253 Deferred（原 grid-snap）+ 本 change Decision 2/3。
- **触发原因（更正）**：深错位退格 floor 需删变量个空格，唯一 redo-免疫解 `Replace(WholeLine)` 光标归位行首
  （同上）。Tab 方向已用 `Insert`（光标正确）落地；退格方向无对称的「删变量个且光标正确」原语。
- **前置依赖**：同上（patch `edit_insert_text` 或多行重构）。
- **触发条件**：用户需要错位缩进一键归正（自动缩进恒产出 4 倍数，实际收益低）。
- **当前 workaround**：`Dedent` 去一级（对齐缩进即到制表位；深错位去一级宽度、非 floor）。
