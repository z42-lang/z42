# Proposal: 独立 native 库机制（编译/链接/运行）+ REPL 层剥离成 dlopen cdylib

## Why

User 目标：把 REPL 的行编辑后端（rustyline + `repl.rs`/`repl_editing.rs`）从核心 z42vm 里**剥离成独立
native 库**，只有真正进 REPL 时才加载，不把 rustyline 及其 19 个传递 crate 塞进每一个 z42vm。并借此
**设计一套「独立 native 库：编译 → 链接 → 运行」的通用机制**——目前只有 compression 一个先例，且其加载
路径是硬编码的 `match name { "compression" => … }`，不成体系。

**诚实的成本/收益（已实测，务必认清）**：

| 维度 | 现状 | 剥离成 cdylib 后 |
|------|------|-----------------|
| z42vm 启动时间 | **已不受影响**（rustyline 懒初始化：`EDITOR.get_or_init` 只在首次 `ReadLine`；启动路径不碰它）| 不变（需**懒 dlopen**，见 design；否则反而更慢）|
| z42vm 核心二进制 | 7.2 MB（含 rustyline + 19 crate）| 缩小 rustyline 那一份（约数百 KB–1 MB）|
| SDK 总体积（磁盘）| 单二进制 | **可能略增**（cdylib 重新内含一份 Rust std/panic 机制，~2 MB dylib）|
| 核心 VM 依赖面 | rustyline + 19 crate 在核心 crate 依赖图内 | **移出核心依赖图**（更干净、构建/审计面更小）——**这是主要真收益** |
| rustyline fork | 需 1 行 patch | **仍需**（光标 bug 在 rustyline 源码，静态/动态都一样）|

**结论（供裁决）**：真收益是**依赖隔离 + 确立通用 native 库范式**，不是戏剧性的体积/启动改善，也**不消除
fork**。是否值得这层 FFI 复杂度，请在看完 design 的「§7 值不值得」后裁决。

## What Changes

1. **通用 native 库机制**（design §1–§5，落文档 `docs/book/`）：cdylib crate 布局 + C ABI 约定 + z42vm 侧
   注册/marshal/dispatch + 发现路径（`Z42_NATIVE_PATH` / `<sdk>/native/`）+ 懒加载 + **re-entrancy 回调
   ABI**（native 库回调 z42 函数，compression 没有、REPL 必须）。
2. **`crates/z42-repl/` 新 cdylib**：搬入 rustyline `Editor` 装配 + `Completer`/`KeyEditHandler` rustyline
   trait 实现；导出 C ABI `z42_repl_readline(prompt, callbacks, ctx)` 等，回调经 C 函数指针 + opaque ctx。
3. **z42vm 侧薄 shim**：`__repl_readline`/`__repl_set_completer`/`__repl_set_key_editor` 保留为**静态
   builtin 薄壳**，首次调用时**懒 dlopen** `libz42_repl`，并提供「回 VM 重入」的 C 回调（VmContext 侧：
   `exec_function` / `ACTIVE_CTX` / GC park / `set_pending_thrown`）。
4. **打包/构建**：xtask + packaging 把 `libz42_repl.{so,dylib,dll}` 产进 `<sdk>/native/`；wasm/mobile 用
   staticlib/rlib（`bundled-repl` feature，镜像 `bundled-compression`）。

**注**：`}`/floor（`add-repl-rbrace-floor`，代码已完成于分支 `add-repl-rbrace-floor`）在本 change 落地后
**rebase 到其上收尾**——它编辑的 `repl.rs`/`repl_editing.rs` 届时已搬进 cdylib。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/crates/z42-repl/Cargo.toml` | NEW | cdylib crate 清单（rustyline 依赖在此，非核心）|
| `src/runtime/crates/z42-repl/src/lib.rs` | NEW | C ABI 导出（`z42_repl_readline`/`_free`/`_last_error`）+ `ReplCallbacks` + Editor 懒建 + span thread-local |
| `src/runtime/crates/z42-repl/src/helper.rs` | NEW | `ReplHelper`（Completer/Hinter/Highlighter/Validator）+ `word_start`——经 `ReplCallbacks.complete` 回调 |
| `src/runtime/crates/z42-repl/src/editing.rs` | NEW | `KeyEditHandler` + `parse_action`——经 `ReplCallbacks.key_edit` 回调 |
| `src/runtime/crates/z42-repl/src/history.rs` | NEW | history 文件加载/保存（`$HOME/.z42_history`）|
| `src/runtime/src/corelib/repl.rs` | MODIFY | 剥掉 rustyline：留 `__repl_*` 薄壳 + 懒 dlopen + `complete_via_callback` 重入 + `complete_trampoline`/`keyedit_trampoline` C 回调 + `plain_readline` fallback + member_names/probe |
| `src/runtime/src/corelib/repl_editing.rs` | MODIFY | 剥掉 `KeyEditHandler`/`parse_action`（→cdylib）；留 `builtin_repl_set_key_editor` + `key_edit_via_callback` 重入 |
| `src/runtime/src/corelib/repl_native.rs` | NEW | 懒 dlopen `libz42_repl`（repl 专用发现路径）+ `ReplCallbacks` 构造 + symbol 缓存 |
| `src/runtime/src/corelib/repl_editing_tests.rs` | MODIFY | `parse_action` 测试随迁 cdylib（这里改留 z42vm 侧 trampoline/marshal 测试）|
| `src/runtime/Cargo.toml` | MODIFY | 删核心 `rustyline` 依赖；加 `crates/z42-repl` 为 workspace 成员；`z42-repl` **不**入 default-members（单独 cdylib 构建）|
| `src/runtime/Cargo.lock` | MODIFY | 重解析 |
| `scripts/build/xtask_toolchain.z42` | MODIFY | 构建 `libz42_repl` cdylib + 拷进 z42i 同侧 toolchain 目录 |
| `scripts/build/xtask_package_desktop.z42` | MODIFY | 打包 `libz42_repl` 进 SDK 的 interactive/toolchain 侧（非 `<sdk>/native/`）|
| `docs/book/src/runtime/native-extensions.md` | NEW | 「独立 native 库：编译/链接/运行」机制 SoT（两类库：runtime 跨平台 vs toolchain host-only）|
| `src/runtime/src/native/README.md` | MODIFY | 指向新机制文档；补 repl(host-only) vs compression(跨平台) 区分 |
| `src/toolchain/repl/README.md` | MODIFY | 功能索引：行编辑后端现为 dlopen cdylib（host-only）|

**只读引用**：`src/runtime/crates/z42-compression/{Cargo.toml,src/lib.rs}` / `src/runtime/src/native/ext.rs`
（cdylib 范式参照）；`add-repl-rbrace-floor` 分支（`}`/floor 待 rebase 迁入 cdylib 的 `editing.rs`/`parse_action`）。

**外部交付物（不在本仓库）**：rustyline fork（`}`/floor 用；本 change 若先落可暂不需要，随 rebase 引入）。

## Out of Scope

- `}`/floor 本身（独立分支 `add-repl-rbrace-floor`，本 change 后 rebase 收尾）。
- 完整「自描述、任意 native 库零 Rust shim」的通用插件 ABI（design §6 的 Option B）——除非 User 明确要，
  默认走 Option A（沿 compression 范式 + 文档化 + 懒加载 + 回调），不投机建大而全的插件系统。
- REPL 目录搬迁（`src/toolchain/repl/` → `interactive/`，另一独立 refactor）。

## Open Questions

- [ ] **§7 值不值得**：真收益（依赖隔离）vs FFI re-entrancy 复杂度 —— User 裁决是否推进。
- [ ] **通用度**：Option A（compression 范式 + 懒加载 + 回调，够 REPL 用）vs Option B（自描述插件 ABI，
      大投资）——design §6。推荐 A。
- [ ] 回调 re-entrancy 的线程/GC-safepoint 语义（rustyline `readline` 阻塞期的 native park）需 design 定清。
