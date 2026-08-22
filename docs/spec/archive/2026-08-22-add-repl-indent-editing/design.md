# Design: REPL 缩进感知行编辑

## Architecture

```
按键 (Backspace / Tab / '}')
      │  rustyline readline() 事件循环 (Rust)
      ▼
ConditionalEventHandler (repl_editing.rs)        ← policy-free 适配壳
      │  取 ctx.line()/ctx.pos() + 键名
      │  经 ACTIVE_CTX 重入 VM (与 replComplete 同款路径)
      ▼
z42  ReplEditing.KeyEdit(key, line, pos) -> string  ← 全部策略在此
      │  返回动作串 ("" / "indent" / "dedent")
      ▼
Rust 解析动作串 -> Option<Cmd>  (None=默认行为；indent/dedent → Indent/Dedent(WholeLine))
      ▼
rustyline 执行 Cmd
```

对称参照：Tab 补全已走 `SetCompleter(fqn)` → `replComplete` 自由函数 → `complete_via_callback` + `ACTIVE_CTX` 重入。本设计把同一模式扩展到键位编辑。

## Decisions

### Decision 1: 决策逻辑落 z42，Rust 只留适配壳
**问题：** 缩进策略放 Rust 还是 z42？
**选项：**
- A（全 Rust）：`repl_editing.rs` 纯函数。简单、快、自足；但策略进 Rust，改行为要 `cargo build`，与既有「sink-to-script」方向相悖。
- B（z42 策略 + Rust 壳）：Rust 只 marshal (line,pos) 回调 z42、翻译动作串。策略在 z42、可 `[Test]`、改行为免 Rust 重建；代价是每键一次 VM 重入 + 总代码略多。
**决定：** 选 B。与 `ContinuationIndent`/`Completer` 同层，符合 User 明确取向；`replComplete` 已证明 readline 中途重入 VM 可靠。

### Decision 2: 动作串协议（z42 → Rust）
**问题：** z42 函数返回不了 rustyline `Cmd`，如何表达编辑动作？
**决定：** 最小字符串协议，Rust 当哑翻译，**仅两个动作、均映射 redo-免疫命令**（见 D6）：
| 动作串 | Cmd |
|--------|-----|
| `""` | `None`（走该键默认） |
| `indent` | `Cmd::Indent(Movement::WholeLine)` |
| `dedent` | `Cmd::Dedent(Movement::WholeLine)` |
z42 `KeyEdit` 只判「要不要成级缩进」；缩进量由 rustyline `config.indent_size()`（设 4）决定。

### Decision 3: 定量一级缩进（放弃网格吸附）
**问题：** 原设计要「吸附到 4 列制表位」（退格 `col%4→` 上制表位，Tab `4-col%4`）。
**决定：** **改为定量一级**（`indent_size=4`）。原因见 D6：变量个字符的删除无法 redo-免疫地实现。
`Cmd::Indent`/`Dedent(WholeLine)` 按 `indent_size` 定量增删——对齐缩进（4/8）即「一级」；错位（如 6）
`Dedent` 去 4 → 2（去「一级宽度」而非吸附）。实测（PTY spike）符合用户核心诉求「退格删缩进 / Tab 插缩进」。

### Decision 4: `}` 自动回退一级 → Deferred
**问题：** `}` 回退需「删一级 + 插 `}`」用**单个** `Cmd`。唯一删+插命令 `Cmd::Replace(mvt, text)` 的
movement 计数会被 redo 覆盖（D6）；`Replace(BeginningOfLine, "}")` 又会删光**全部**前导空白（嵌套深度 >1 时过删）。
**决定：** 本次 **Deferred**（`}` 回退本就是原 `Completeness.z42` 注释里标注的「后续细化」）。见 Deferred 段。

### Decision 6: rustyline redo 覆盖 movement 计数（spike 发现，本 change 关键约束）
**问题：** 首版实现 `Cmd::Kill(BackwardChar(4))` 退格只删了 1 个字符（PTY spike 实测 + KEYDBG 确认
handler 正确返回 `kill 4`，但生效为删 1）。
**根因：** rustyline `emacs()` 对自定义绑定返回的**可重复**命令执行 `cmd.redo(Some(n))`，`n` = 数字前缀
（普通按键 = 1）；`Movement::BackwardChar(previous).redo(Some(1))` = `BackwardChar(1)`——嵌入的计数被 n 覆盖。
Tab 的 `Insert(1, "    ")` 不受影响，因 4 空格在 **text** 里（count 是「重复 text 次数」）。
**决定：** 删除类一律走 redo-免疫的 `Cmd::Dedent(WholeLine)`（`WholeLine` 无计数、redo 返回自身不变；量取
`config.indent_size`）。同理 Tab 用 `Cmd::Indent(WholeLine)`（与退格对称、可预测）。这直接决定了 D2/D3/D4。

### Decision 5: 文件拆分（500 行硬限）
**问题：** `repl.rs` 已 468 行，加壳会越 500。
**决定：** 新建 `repl_editing.rs` 放 handler + 动作串解析；`repl.rs` 只加 builtin + `bind_sequence` 调用。

### Decision 6: None 落回默认
**问题：** Tab 返回 `None` 时是否落回补全？
**决定：** rustyline `ConditionalEventHandler` 文档：返回 `None` = 执行该键**默认**命令。Tab 默认即补全。实现后 spike 验证若 `None` 未落回补全，则壳显式返回 `Cmd::Complete`。

## Implementation Notes

- **重入安全**：handler 内取 `ACTIVE_CTX`（`read_one_line` 已在 readline 全程 set/clear），临时 un-park GC（与 `Completer::complete` 同款），调 `KeyEdit`。
- **键绑定**：`ed.bind_sequence(KeyEvent(KeyCode::Backspace, M::NONE), EventHandler::Conditional(...))`，Tab 同理。**不绑 `}`**（D4 Deferred）。
- **config**：`Config::builder().completion_type(List).indent_size(4)`——`indent_size` 即 Indent/Dedent 的一级量。
- **z42 `KeyEdit` 纯函数**：字符串扫描当前逻辑行（末个 `\n` 后）判空白前缀、算 col、按键名分派返回 indent/dedent/""。无副作用、可 golden 测。
- **redo-免疫**：仅用 `Cmd::Indent`/`Dedent(WholeLine)`（`WholeLine` 无计数）——见 D6，避开 `redo(Some(n))` 覆盖 movement 计数。

## Testing Strategy

- **z42 golden**（`tests/repl_editing/` driver.z42 + expected_output.txt）：`KeyEdit` 全场景——退格/Tab × 对齐/错位/多行/非空白前缀/col0/行首/有词。策略的权威测试。**手动跑通**（scripting/interactive 不进自动 GREEN gate，见 memory green-gate-skips-scripting-interactive）：组装 Z42_LIBS（stdlib+z42c dist）→ z42c build driver → z42vm 运行 → diff。**实测 14/14 匹配**。
- **Rust 单测**（`repl_editing_tests.rs`）：`parse_action` → `Cmd::Indent`/`Dedent`/`None`。**cargo test 4/4**。
- **PTY spike**（交互验收 + #5）：python pty 拉起 z42i，实测 **退格删一级（4 空格 → 0）✓、Tab 加一级（0 → 4 空格）✓**；#5 粘贴刻画（见 Deferred repl-indent-future-paste-reflow）。每键回调延迟：与 Tab 补全同一重入路径、golden 19 调用瞬时——无感（非问题）。
- **GREEN**：`cargo build` + `cargo test --lib` + `xtask test` 全 stage（自举 5/5 gen1==gen2）。

## Deferred / Future Work

### repl-indent-future-multiline-editing: 真·整块多行编辑
- **来源**：本 change 探索阶段（#4）
- **触发原因**：需把完整性判定下沉为 rustyline `Validator`，重构 `Script.z42` 逐行累积循环——结构性大改。
- **前置依赖**：本 change 的键位基础；对累积/completeness 架构的重新设计。
- **触发条件**：用户需要「方向键回上一行改代码」「跨行退格」「粘贴不乱」的整块编辑体验。
- **当前 workaround**：逐行累积 + 预填缩进 + 本 change 的缩进键位。

### repl-indent-future-paste-reflow: 多行粘贴重排
- **来源**：本 change #5 spike（PTY 实测）
- **spike 结论**：bracketed paste 默认开；多行粘贴**整体进单个编辑缓冲**（带内嵌 `\n`，跨视觉行显示），
  粘贴内容**未被二次缩进**（自动缩进预填只作用于手打续行，不碰粘贴）；不自动提交，等 Enter 才作为一条
  readline 结果返回。→ **无双重缩进/损坏，比预想的轻**；不需要紧急修。
- **触发原因**：真正的缺口是「粘贴后无法回改任意一行」——属逐行 readline 架构，真修依赖多行编辑重构。
- **触发条件**：用户频繁粘贴大代码块并需就地编辑时。
- **当前 workaround**：粘贴可用（无损坏）；逐行手打续行走自动缩进 + 本 change 的缩进键位。

### repl-indent-future-rbrace-dedent: `}` 自动回退一级
- **来源**：本 change #3（原 `Completeness.z42` 注释标注的「auto-dedent-on-`}` 属后续细化」）
- **触发原因**：需「删一级 + 插 `}`」单命令；rustyline `Replace` 的 movement 计数被 redo 覆盖（D6），
  `Replace(BeginningOfLine,"}")` 又会删光全部前导空白（嵌套深度 >1 过删）。
- **前置依赖**：可控计数的删+插原语（需 patch rustyline 或走多行编辑重构）。
- **触发条件**：多行编辑重构落地后一并做，或用户明确要求。
- **当前 workaround**：`}` 起头的续行手动退格一次（退格已成级 dedent，一次到位）。

### repl-indent-future-grid-snap: 缩进网格吸附
- **来源**：本 change D1/D3（原设计目标，实现期因 redo 限制降级为定量一级）
- **触发原因**：网格吸附需删/插**变量**个空格；rustyline redo 强制 `redo(Some(1))` 覆盖 movement 计数（D6）→
  变量计数删除退化成删 1，redo-免疫做不到。
- **前置依赖**：patch rustyline（自定义绑定不走 redo，或 Movement 计数不被覆盖）。
- **触发条件**：用户明确需要错位缩进一键归正时（自动缩进恒产出 4 的倍数，网格吸附实际收益低）。
- **当前 workaround**：定量一级 `Indent`/`Dedent`（对齐缩进即一级；错位去/加一级宽度）。
