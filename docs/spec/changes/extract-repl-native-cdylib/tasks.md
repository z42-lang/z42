# Tasks: 独立 native 库机制 + REPL cdylib 剥离

> 状态：🟡 进行中 | 创建：2026-08-29 | Option A（compression 范式 + 懒加载 + 回调），host-only toolchain 打包

## 进度概览
- [x] 阶段 1: z42-repl cdylib crate（rustyline 下沉 + C ABI）
- [x] 阶段 2: z42vm 侧薄壳 + 懒 dlopen + re-entrancy trampoline
- [ ] 阶段 3: 打包（build + ship 进 interactive/toolchain 侧）+ 发现路径
- [ ] 阶段 4: 测试 + 文档 + GREEN
- [ ] 阶段 5: rebase add-repl-rbrace-floor 收尾（}/floor 进 cdylib）

## 阶段 1: cdylib crate
- [x] 1.1 `crates/z42-repl/Cargo.toml`（crate-type cdylib+rlib；rustyline 依赖；无 z42 主 crate 依赖）
- [x] 1.2 `src/lib.rs`：`ReplCallbacks` C 结构 + `z42_repl_readline`/`_free`/`_last_error` + Editor 懒建 + span thread-local(CBS)
      —— 阶段 2 追加 `Z42_REPL_NO_EDITOR=4` out_kind（no-tty → z42vm plain fallback，不当错误）
- [x] 1.3 `src/helper.rs`：`ReplHelper`（Completer/Hinter/Highlighter/Validator）+ `word_start`（经 cbs.complete）
- [x] 1.4 `src/editing.rs`：`KeyEditHandler` + `parse_action`（经 cbs.key_edit）
- [x] 1.5 `src/history.rs`：history 文件加载/保存
- [x] 1.6 `cargo build -p z42-repl`（cdylib 独立编过）+ `cargo test -p z42-repl`（7 绿）

## 阶段 2: z42vm 侧
- [x] 2.1 `repl.rs`：剥 rustyline（ReplHelper/Completer/EDITOR/word_start/history/ACTIVE_CTX）；留 builtins +
      `complete_via_callback` + `complete_trampoline` + `plain_readline`(pub(crate)) + member_names/probe
- [x] 2.2 `repl_editing.rs`：剥 KeyEditHandler/parse_action；留 `builtin_repl_set_key_editor` +
      `key_edit_via_callback` + `keyedit_trampoline`
- [x] 2.3 `repl_native.rs`（NEW）：懒 dlopen `libz42_repl`（`Z42_REPL_NATIVE` → 运行二进制同侧 bin/）+
      `ReplCallbacks` 构造 + OnceLock symbol 缓存 + `z42vm_free_str` + 缺库/no-tty/no-native-interop plain fallback。
      trampoline/dl 内部 gated on `feature="native-interop"`；模块 gated on `not(wasm)`
- [x] 2.4 `Cargo.toml`：删核心 rustyline（连带 ~19 传递 crate）；`crates/z42-repl` 已是 workspace 成员（非 default-members）
- [x] 2.5 `cargo build`（核心 z42vm 不再含 rustyline，无 repl 警告）+ `cargo test --lib`（1000 绿/2 ignored）+
      `cargo check --no-default-features --features interp-only`（plain fallback 路径也编过）+ nm 验 3 个 C ABI 符号导出

## 阶段 3: 打包 + 发现
- [x] 3.1 **dev 流不建 cdylib**（与 compression 一致：`scripts/build/` 从不建 native cdylib，dev 靠开发者
      `cargo build -p z42-repl` 落 target/ 同侧、被 `repl_native` 探到）——故**不改** `xtask_toolchain.z42`，
      避免与 compression 不一致
- [x] 3.2 SDK 打包（`xtask_stage_components.z42`）：`_pkgBuildAndStageRuntime` 加 `_cargo(... "z42-repl")` 建
      cdylib；`_pkgStageZ42vm` 把 `libz42_repl.{dylib,so}`/`z42_repl.dll` 拷进 **z42vm 组件 bin/**（与 z42i/z42vm
      同侧，`[assemble]` 自动并入 pkgDir/bin/）；`_copyNativeLibs` 显式**排除** `libz42_repl`/`z42_repl`（否则
      glob `libz42*` 会误拷进 `native/`）
- [x] 3.3 发现路径与打包位置对齐：`repl_native::candidates()` = `current_exe().parent()/libz42_repl.*`，SDK 里
      z42vm/z42i 都在 `bin/`、lib 也在 `bin/` → 同侧解析。**SDK 实际运行验证留 CI/dist smoke（3.x 本地不可跑）**

## 阶段 4: 测试 + 文档 + GREEN
- [ ] 4.1 cdylib 单测（parse_action / word_start / Completer 逻辑，mock cbs）
- [x] 4.2 z42vm trampoline 单测（新 `repl_tests.rs`/`repl_editing_tests.rs`/`repl_native` tests，10 个）：
      complete/keyedit trampoline 的**防御契约**——null ctx/line/key → null、无注册 → null、callback 出错
      （module=None）→ **吞成 null 不 panic**（park 嵌套镜像 builtin_repl_readline 保 parked_count 平衡）+
      `z42vm_free_str` 释放/null no-op + `lib_filename` 平台形状。re-entrancy 正确性由 `__repl_complete_probe`
      路径 + z42 golden 覆盖（未变）。
      —— 注：阶段 2 删的旧 `repl_tests.rs`(word_start)/`repl_editing_tests.rs`(parse_action) 已随代码搬进 cdylib
      由 `cargo test -p z42-repl` 覆盖；本 4.2 是**新**的 VM 侧 trampoline 覆盖，与旧文件同名但内容全换。
- [x] 4.3 `docs/book/src/runtime/native-extensions.md`（通用机制 SoT，含单向/compression + 双向/repl 两实例、
      ReplCallbacks/trampoline/out_kind/懒 dlopen/GC 重入/打包位置 + 新增扩展清单）+ SUMMARY 接线。
      crate README 不加（与 compression 一致，靠 lib.rs `//!` doc；native/README 无此文件，机制 SoT 已覆盖）
- [ ] 4.4 `xtask test` + dist smoke `z42 repl -c "1+2"` + 交互验收（补全/键位）
- [ ] 4.5 cold/bootstrap/wasm(bundled 无 repl，plain fallback) 交 CI

## 阶段 5: rebase }/floor
- [ ] 5.1 rebase `add-repl-rbrace-floor` 于本 change 之上：`}`/floor 进 cdylib `editing.rs`/`parse_action`；rustyline fork 的 `[patch.crates-io]` 落 `crates/z42-repl`（cdylib 才依赖 rustyline）

## 备注
- 零 zbc/zpkg 格式 bump（纯 runtime 结构）。
- `Std.Repl` z42 源 + `__repl_*` 名不变 → stdlib/toolchain 零改。
- 关键不变量：z42 内部类型（Value/VmContext）绝不过 C 边界；marshal + VM 重入留 z42vm trampoline。
