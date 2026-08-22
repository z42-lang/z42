# Tasks: REPL 缩进感知行编辑

> 状态：🟢 已完成 | 创建：2026-08-22 | 完成：2026-08-22 | 定级：feat(toolchain) 轻量流程

## 进度概览
- [x] 阶段 1: z42 策略层（KeyEdit + golden）
- [x] 阶段 2: z42 接线（SetKeyEditor 注册）
- [x] 阶段 3: Rust 适配壳（重入 handler + 动作串解析 + builtin + 绑键）
- [x] 阶段 4: #5 粘贴 spike + 每键延迟结论
- [x] 阶段 5: 验证与文档

## 阶段 1: z42 策略层
- [x] 1.1 `ReplEditing.z42`：`KeyEdit(key,line,pos)`——纯缩进行判定 → 返回 `indent`/`dedent`/`""`（量由 rustyline `indent_size` 定，见 D6 redo 约束）
- [x] 1.2 `tests/repl_editing/`：golden 覆盖退格/Tab × 对齐/错位/多行/非空白/col0/有词（14 场景，实测匹配）

## 阶段 2: z42 接线
- [x] 2.1 `Repl.z42`：加 `SetKeyEditor(string fqn)` → `__repl_set_key_editor`
- [x] 2.2 `interactive_main.z42`：REPL 启动注册 `Repl.SetKeyEditor("Std.Scripting.replKeyEdit")`

## 阶段 3: Rust 适配壳
- [x] 3.1 `repl_editing.rs`：`parse_action`（indent/dedent → `Cmd::Indent`/`Dedent(WholeLine)`）+ `repl_editing_tests.rs`（4/4）
- [x] 3.2 `repl_editing.rs`：通用 `KeyEditHandler`（取 line/pos + 键名 → 回调 z42 → 解析）
- [x] 3.3 `repl.rs`：`active_ctx_ptr` 访问器（供 sibling 模块读 readline-span ctx）
- [x] 3.4 `repl.rs`：`read_one_line` 设 `indent_size(4)` + `bind_sequence` 挂 Backspace/Tab（`}` Deferred）
- [x] 3.5 `mod.rs`：声明 `repl_editing` 模块 + 注册 `__repl_set_key_editor` builtin

## 阶段 4: #5 粘贴 spike
- [x] 4.1 PTY 实测多行粘贴：bracketed paste 默认开；整体入单缓冲、**无双重缩进**、不自动提交（结论入 design.md Deferred repl-indent-future-paste-reflow）
- [x] 4.2 每键回调延迟：与 Tab 补全同一重入路径、golden 瞬时 → 无感，非问题
- [x] 4.3 结论：粘贴真修（回改任意粘贴行）依赖多行编辑重构 → Deferred

## 阶段 5: 验证与文档
- [x] 5.1 `cargo build --release`（z42vm）
- [x] 5.2 `cargo test --lib`（repl_editing 4/4）
- [x] 5.3 `xtask test`（全 stage GREEN；自举 5/5 gen1==gen2）
- [x] 5.4 交互验收（PTY）：退格删一级 ✓、Tab 加一级 ✓
- [x] 5.5 spec scenarios 逐条覆盖（golden 14/14）
- [x] 5.6 文档同步：`scripting/README.md`（功能索引 + 核心文件）

## 备注
- **关键发现（spike）**：rustyline 对自定义绑定的可重复命令执行 `redo(Some(n=1))`，覆盖 movement 计数 →
  `Kill(BackwardChar(4))` 退化成删 1。改用 redo-免疫的 `Cmd::Indent`/`Dedent(WholeLine)`（按 `indent_size` 定量）。
  连带：网格吸附 + `}` 回退一级 Deferred（见 design.md Deferred 段）。
- 交互层（z42.scripting/z42.interactive）无 [Test] → 不进自动 GREEN gate；本次靠 `xtask build toolchain`
  编译验证 + 手动 golden + PTY 交互验收（green-gate-skips-scripting-interactive）。
