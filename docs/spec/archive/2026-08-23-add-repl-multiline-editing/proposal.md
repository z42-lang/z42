# Proposal: REPL 整块多行编辑（whole-buffer multiline）

## Why

当前 REPL 是**逐行 readline 架构**：`Repl.ReadLine` 每次只读一个物理行（一次 rustyline
`ed.readline()` 调用），多行语句由脚本层 `interactive_main.z42` 累积进 `buf`，靠
`Completeness.IsIncomplete(buf)` 判「写完没」。每个物理行是独立的 readline，**上箭头是历史、不是
当前多行语句的上一行** → 一旦回车进了续行，前面的行就再也改不到。

三个直接后果（均为 [[add-repl-indent-editing]] 归档时登记的 Deferred）：

1. **粘贴后无法回改任意行**：粘贴一段多行代码，发现第 2 行打错，只能 Ctrl-C 整段作废重来。
2. **续行中方向键不能跨行导航**：无法上下移动到本语句的其它行编辑。
3. **`}` 自动回退一级 / 退格深错位 floor 难以干净实现**：逐行模型下这些要 `Replace(WholeLine)`，
   撞 rustyline `edit_insert_text` 光标不推进的坑（见 add-repl-tab-grid-snap Deferred）。整块缓冲
   模型下光标处理天然正确，为这几项提供干净地基。

整块多行编辑是这三项的**共同地基**，也是 REPL 编辑体验的最大单一杠杆。

## What Changes

- **改读取模型**：`Repl.ReadLine` 从「读一物理行 + initial 预填」升级为「读**整块多行语句**」——
  一次 `ed.readline()` 调用覆盖整条语句；rustyline 在语句未写完时插入换行并保持整个缓冲可编辑
  （方向键跨行、可回改任意行），写完才提交。
- **回车语义交 z42 权威判定**：新增「回车键」重入回调——按下 Enter 时把**整个缓冲**交
  `Completeness.IsIncomplete` 判定：未写完 → 插入 `\n` + `ContinuationIndent` 计算的缩进（光标落其后）；
  写完 → 提交整块。沿用现有 `replComplete` / `replKeyEdit` 的「逻辑在 z42、Rust 只翻译动作串」重入范式。
- **脚本层循环塌缩**：`interactive_main.z42` 删除 per-line 累积（`buf` + `while` 续读 + `initial`
  预填），改为「一次 ReadLine 拿整条语句 → 求值」。
- **Deferred 地基**：`}` auto-dedent / 退格 floor 在整块模型下变为可干净实现（本 change 不做它们本身，
  只把地基铺好并在 design 说明后续如何接）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/repl.rs` | MODIFY | `read_one_line` 接 Validator / 绑定 Enter 键重入 handler；`ReplHelper::Validator` 落地（当前空 stub）；`initial` 预填路径按新模型调整 |
| `src/runtime/src/corelib/repl_editing.rs` | MODIFY | Enter handler（复用 `KeyEditHandler` 重入范式）；`parse_action` 扩「accept」/「newline:<indent>」动作 |
| `src/runtime/src/corelib/repl_editing_tests.rs` | MODIFY | 新动作串的 `parse_action` 单测 |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 若新增 builtin（如 `__repl_set_enter_editor`）则注册 |
| `src/toolchain/scripting/src/ReplEditing.z42` | MODIFY | 新增 Enter 策略函数（判 accept vs newline+indent，复用 Completeness）；或新文件承载 |
| `src/toolchain/scripting/src/Repl.z42` | MODIFY | `ReadLine` 语义升级为整块读取；新增 `SetEnterEditor`（若走注册模型） |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 主循环塌缩：删 per-line 累积 / `buf` / initial 续读，改整块读取 |
| `src/toolchain/scripting/tests/repl_editing/driver.z42` | MODIFY | 新增 Enter 策略（accept/newline）纯函数 golden 用例 |
| `src/toolchain/scripting/tests/repl_editing/expected_output.txt` | MODIFY | 对应期望输出 |
| `src/runtime/src/tests/` 或 `src/toolchain/scripting/tests/` | NEW | 整块多行的 PTY / e2e 交互回归（粘贴回改、跨行导航、提交时机） |
| `docs/book/src/**/repl-input-completeness.md`（或相邻页） | MODIFY | 记整块多行编辑机制 + 回车判定数据流 + Deferred 地基 |
| `docs/roadmap.md` | MODIFY | Deferred 表：本 change 解锁「粘贴回改」/「跨行导航」，更新 `}`/floor 依赖说明 |

**只读引用：**

- `src/toolchain/scripting/src/Completeness.z42` — `IsIncomplete` / `ContinuationIndent`（整块判定/缩进的现成入口）
- `src/toolchain/scripting/src/Completer.z42` — 补全器重入范式参照
- rustyline 14 `Validator` / `AcceptOrInsertLine` / `ValidationResult` API（外部 crate，理解多行提交机制）

## Out of Scope

- **`}` 自动回退一级、退格深错位 floor 本身**：本 change 只铺地基，这两项作为后续细化（design 说明接法）。
- **fork / vendor / `[patch]` rustyline**：本 change 严格用 stock rustyline 14（Validator 是公开 trait）。
- **语法完整性判定算法**：沿用 `add-repl-parser-completeness` 的 parser 权威判定，不改 `Completeness` 逻辑。
- **`-c "expr"` 单次求值路径**：保持不变（无交互来源，仍靠 `IsIncomplete` 判非法即退）。

## Open Questions

- [ ] **OQ1 回车机制选型**：自定义 Enter handler（z42 返回 accept/newline 动作串，Rust 翻译，含缩进控制）
      vs 纯 rustyline `Validator`（返回 Incomplete 让 rustyline 自插换行，但**无自动缩进**）。
      推荐前者（缩进可控 + 与现有 keyedit 范式一致）——待 design 定稿 + User 确认。
- [ ] **OQ2 Enter 位于缓冲中段的语义**：光标不在末尾时按 Enter，是「拆行插入」还是「若整体完整则提交」？
      rustyline `AcceptOrInsertLine { accept_in_the_middle: false }` = 仅末尾且完整才提交，否则插行。
      推荐此语义（编辑器直觉）。
- [ ] **OQ3 Ctrl-C / Ctrl-D 在整块模型下的语义**：当前「续读中 Ctrl-C 弃缓冲回主提示符」。整块模型下
      一次 readline 即整条，Ctrl-C 由 rustyline 直接中断该 readline → 返回 null → 回主提示符。语义等价，
      需确认无回退。
- [ ] **OQ4 `.` 元指令与多行**：元指令只在首行、单行完整。整块模型下需在整块读取**之前/之中**识别首行
      `.` 前缀并短路（不进入多行续读）。design 给判定点。
