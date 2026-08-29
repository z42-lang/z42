# Design: REPL `}` 自动回退 + 退格 floor（rustyline patch 承载）

## Architecture

沿用既有三层（决策在 z42、翻译在 Rust、机制在 rustyline）：

```
按键 } / Backspace
  └─ repl.rs 键绑定 → KeyEditHandler("rbrace" | "backspace")
       └─ 重入 VM → Std.Repl.ReplEditing.KeyEdit(key, line, pos)   [策略，纯计算]
            └─ 返回动作串 "replace:<text>" / "dedent" / ""
       └─ parse_action → Cmd::Replace(WholeLine, Some(text))       [翻译]
            └─ rustyline: edit_kill(WholeLine)=move_home+kill_line  → 光标行首
                          edit_insert_text(text) → insert_str + **set_pos(cursor+len)**  [fork patch]
                          → 光标落在 text 末尾（`}` 之后 / 缩进之后）
```

## Decisions

### Decision 1: 为什么必须 patch rustyline（route B 已证伪）

**问题**：`}` 需「dedent + 插 `}` + 光标正确」，退格 floor 需「变量宽度反向删 + 光标正确」；一次按键
只产**一个** `Cmd`。

**已在 rustyline 14.0 源码核实**（只读引用）：

- 自定义绑定返回的可重复 `Cmd` 必经 `cmd.redo(Some(n))`（`keymap.rs:526`），`n`=数字前缀=1；
  `repeat_count(prev, Some(1))=1` → `Kill(BackwardChar(delta))` 退化成 `BackwardChar(1)`。**变量宽度
  反向删无法 redo-免疫。**
- 唯一 redo-免疫的变量宽度删+插 = `Replace(WholeLine, text)`（`WholeLine.redo` 恒等、text 在 payload）。
  但 `Replace` = `edit_kill`（`move_home`→光标行首）+ `edit_insert_text`；`edit_insert_text`→
  `line_buffer.insert_str` **不改 `pos`**（`edit.rs:545` / `line_buffer.rs:881`）→ 光标停行首，破坏
  `} else {`。

**决定**：patch `edit_insert_text` 使插入后推进光标。**安全性核实**：`edit_insert_text` 在整个
rustyline crate 内**唯一调用方**是 `command.rs:66` 的 `Cmd::Replace` 路径；推进光标是 emacs/vi 下
replace/yank 的正确语义。故 patch 只令 `Replace` 光标正确、不改任何其它命令 —— 是上游真 bug，可同步上游。

### Decision 2: patch 内容与承载方式

**patch**（rustyline `src/edit.rs::edit_insert_text`）：

```rust
pub fn edit_insert_text(&mut self, text: &str) -> Result<()> {
    if text.is_empty() { return Ok(()); }
    let cursor = self.line.pos();
    self.line.insert_str(cursor, text, &mut self.changes);
    self.line.set_pos(cursor + text.len());   // ← 新增：推进光标到插入文本之后
    self.refresh_line()
}
```

`line_buffer.set_pos` 已存在（`line_buffer.rs:138`）。`text.len()` 是字节长度，与 `pos`（字节偏移）
同度量；替换文本为「空格 + `}`」纯 ASCII，无 UTF-8 边界问题。

**承载**（rebase on extract-repl-native-cdylib）：rustyline 依赖现在只在 host-only cdylib
`src/runtime/crates/z42-repl/Cargo.toml`（`rustyline = "14"`），核心 VM 已去 rustyline。但
`[patch.crates-io]` 只在 **workspace 根** 生效，故 fork patch 仍写在 `src/runtime/Cargo.toml`：

```toml
[patch.crates-io]
rustyline = { git = "https://github.com/z42-lang/rustyline", branch = "z42-edit-insert-text-cursor" }
```

fork = rust-lang/rustyline v14.0.0 + 上述单 commit。CI 经公网 git 拉取（github 公开 repo）。同步向上游
提 PR，合并进 rustyline 后即撤 fork、回落纯 crates.io 版本。

**选项 A（fork，采纳）** vs B（in-tree vendor 整份源码）：User 定 A —— 单 commit 的 fork 比 vendor 一整
crate 维护面小，且便于上游化。

### Decision 3: `}` / 退格 floor 的策略语义（z42 侧）

介入的**充要条件**：当前逻辑行（末个 `\n` 之后到下个 `\n` 或缓冲末尾）**整行纯空白**。`Replace(WholeLine)`
会替换整条逻辑行，故必须确保被替换内容无有意义字符（否则破坏用户输入）。有词/有内容 → 返回 `""` 走默认。

设 `col` = 光标前空白列数，`indent = 4`：

- **`}` 回退**：目标缩进 = `max(0, floorToStop(col) - indent)`，`floorToStop(col) = (col/indent)*indent`。
  返回 `"replace:" + spaces(目标) + "}"`。例：col=8→`    }`（4 空格）；col=4→`}`；col=6→`}`（floor 到 4 再退一级=0）；col=0→`}`。
- **退格 floor**：目标 = 前一制表位 = `((col-1)/indent)*indent`（col>0）。返回 `"replace:" + spaces(目标)`。
  例：col=6→4（删 2）；col=8→4（删 4，与旧 `Dedent` 同）；col=4→0；col=3→0。**对齐缩进与旧行为一致，
  仅错位时改为归正。**

**退格何时用 floor vs 旧 `dedent`**：仅当整行纯空白（`Replace(WholeLine)` 安全）时用 `replace:` floor；
否则（前缀空白但行内光标后有内容）保持旧 `"dedent"`（`Dedent(WholeLine)` 只删行首 indent、不碰光标后内容）。

### Decision 4: `}` 键绑定与 Shift 归一

cdylib `lib.rs::build_editor` 加 `ed.bind_sequence(KeyEvent(KeyCode::Char('}'), Modifiers::NONE), Conditional(KeyEditHandler("rbrace")))`。
rustyline 对可打印字符把 Shift 折进字符本身（`}` 即 `Char('}')`、Modifiers::NONE），无需单列 Shift 修饰。
handler 未注册 / 非空白行 / 无 live ctx → 返回 `None` → 默认插入 `}`，正常输入永不受阻。

## Implementation Notes

- `ReplEditing.z42` 加辅助：取当前逻辑行 `[lineStart, lineEnd)` 并判整行纯空白；`floorToStop` / 前制表位算列。
- `parse_action` 加 `_ if s.starts_with("replace:") => Some(Cmd::Replace(Movement::WholeLine, Some(...)))`。
  `replace:` 后是字面文本（空格 + 可选 `}`），无转义、绝不含冒号歧义（与 `insert:` 同约定）。
- 头注（cdylib `editing.rs`）：把「Deferred：Replace 光标 bug」段改为「已 patch，`replace:` 落地 `}`/floor」。
- **key 名** 用 `"rbrace"`（非字面 `}`，避免动作串/键名歧义）。

## Testing Strategy

- **Rust 单元**（cdylib `editing.rs` 内联 `#[cfg(test)]`，`cargo test -p z42-repl` 本机可跑）：
  `parse_action("replace:    }")` → `Cmd::Replace(WholeLine, Some("    }"))`；`parse_action("replace:")` → `Replace(WholeLine, Some(""))`。
- **z42 [Test] golden**（`tests/repl_editing/driver.z42`）：`KeyEdit("rbrace", "        ", 8)` → `"replace:    }"`；
  `KeyEdit("rbrace", "    ", 4)` → `"replace:}"`；`KeyEdit("backspace", "      ", 6)` → `"replace:    "`；
  `KeyEdit("backspace", "        ", 8)` → `"replace:    "`；非空白行 → `""`。
- **光标 patch 正确性**：在 fork 的 rustyline 测试内加一条 `Replace` 后 `pos` 断言；REPL 端到端手感靠交互
  验收（拷新鲜 zpkg 进 `.z42/libs`）。
- **GREEN**：`cargo build` + `xtask test`（runtime 改动另 `cargo test --lib`）；cold/bootstrap 交 CI。
  **零 zbc/zpkg 格式 bump**（纯 VM 行为 + 策略，不动格式）。
