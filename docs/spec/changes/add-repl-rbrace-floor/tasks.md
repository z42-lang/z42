# Tasks: REPL `}` 自动回退 + 退格 floor

> 状态：🟢 代码完成，rebase 到 extract-repl-native-cdylib 之上 | 创建：2026-08-29
>
> **落点迁移**：REPL 行编辑后端已在 PR #325 剥离成 host-only cdylib `src/runtime/crates/z42-repl/`。
> 本变更的 `parse_action` / `}` 键绑定 / `replace:` 单测均落进 **cdylib**（`editing.rs` / `lib.rs`），
> 不再在 VM 侧 `corelib/repl*.rs`；`[patch.crates-io]` 仍在 workspace 根 `src/runtime/Cargo.toml`。

## 进度概览
- [x] 阶段 1: rustyline fork + patch
- [x] 阶段 2: 核心实现（策略 + 适配壳 + 键绑定）
- [x] 阶段 3: 测试与文档（cargo 侧本地绿；完整 GREEN 交 CI）

## 阶段 1: rustyline fork + patch
- [x] 1.1 fork 托管 `z42-lang/rustyline`
- [x] 1.2 fork 内 patch `src/edit.rs::edit_insert_text`：插入后 `set_pos(cursor + text.len())`（分支 `z42-edit-insert-text-cursor` commit `0f70b80f`）
- [x] 1.3 `src/runtime/Cargo.toml`（workspace 根）加 `[patch.crates-io] rustyline = { git=..., branch=... }`
- [ ] 1.4 （并行，可选）向上游 rustyline 提 `edit_insert_text` 光标 PR（合并后可撤 fork）

## 阶段 2: 核心实现
- [x] 2.1 `ReplEditing.z42`：当前逻辑行 `LineEnd` 辅助 + 整行纯空白判定；floor/前制表位算列
- [x] 2.2 `ReplEditing.z42`：`key=="rbrace"` 分支 → `"replace:"+spaces(目标)+"}"`（整行纯空白+光标行尾时）
- [x] 2.3 `ReplEditing.z42`：`key=="backspace"` 整行纯空白+光标行尾+缩进错位 → `"replace:"+spaces(floor)`；否则保留 `"dedent"`
- [x] 2.4 cdylib `editing.rs`：`parse_action` 加 `replace:` → `Cmd::Replace(WholeLine, Some(text))`；更新头注（删 Deferred 段）
- [x] 2.5 cdylib `lib.rs::build_editor`：绑定 `}` 键（`KeyCode::Char('}')`）→ `KeyEditHandler::new("rbrace")`
- [x] 2.6 cdylib `editing.rs` 内联 `#[cfg(test)]`：`replace:` 解析单测

## 阶段 3: 测试与验证
- [x] 3.1 `tests/repl_editing/driver.z42` + `expected_output.txt`：`}`/floor [Test] 断言（各 scenario）
- [x] 3.2 `cargo test -p z42-repl`（含 parse_action `replace:` 单测 + fork patch 生效）
- [ ] 3.3 `xtask test`（完整 GREEN gate）— 交 CI（本机 z42vm 退出挂起 + 种子墙）
- [x] 3.4 spec scenarios 逐条覆盖确认
- [x] 3.5 文档同步：book `repl-input-completeness.md`；`repl/README.md` 功能索引；`docs/roadmap.md` 关闭 Deferred 行
- [ ] 3.6 交互验收（拷新鲜 zpkg 进 `.z42/libs`，手测 `}`/退格手感）— 交 CI/dist smoke
- [ ] 3.7 cold/bootstrap 交 CI 盯绿（零格式 bump）

## 阶段 4: REPL 目录搬迁（Scope 追加，User 授权并入本 PR）
- [x] 4.1 `git mv src/toolchain/repl → src/toolchain/interactive/repl`（z42.repl 独立包，物理移入 interactive 目录）
- [x] 4.2 `scripts/build/xtask_toolchain.z42`：`_buildReplLib` 构建路径 → `src/toolchain/interactive/repl/`
- [x] 4.3 活文档路径：book `repl-input-completeness.md` 代码指针；本 change proposal 的 Scope 表；archive 冻结不动
- [ ] 4.4 CI 验：`compile-toolchain` + `test-host`（toolchain 构建路径生效）——本机 z42vm 挂起，交 CI

## 备注
- 零 zbc/zpkg 格式 bump（纯 VM 行为 + 策略 + 目录搬迁）。
- 搬迁作**独立 commit**（refactor 与 feature 分提交，见 commit-log.md）；deps 按名解析，无依赖图变更。
