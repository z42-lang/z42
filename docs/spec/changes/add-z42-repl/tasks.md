# Tasks: z42 原生交互式 REPL

> 状态：🟠 6.5 已通过（2026-07-23：D2=A / D3=A / 结果打印 1 层），实现待锁释放排队 | 创建：2026-07-23
> 子系统锁（2026-07-23 实测）：`runtime`（被 `lazy-per-function-jit` 占）‖ `stdlib`（被 `converge-z42c-onto-z42-project` 占）‖ `toolchain`（空闲）
> **跨子系统 change → 需同时占三把锁；现 runtime+stdlib 被占 → 排队**（或 User 授权隔离分支并行，参照 stabilize-dispatch-keys 先例）。
> 实现顺序（锁齐后）：runtime（阶段 1）→ stdlib（阶段 2）→ toolchain（阶段 3）

## 进度概览
- [ ] 阶段 1: VM builtin（runtime 锁）
- [ ] 阶段 2: z42.scripting 库（stdlib 锁）
- [ ] 阶段 3: z42.interactive REPL 宿主 + launcher 路由（toolchain 锁）
- [ ] 阶段 4: 测试与验证
- [ ] 阶段 5: 文档同步

## 阶段 1: VM builtin（runtime）
- [ ] 1.1 `src/runtime/Cargo.toml` 加 rustyline 依赖
- [ ] 1.2 `src/runtime/src/corelib/repl.rs` 新建：`__repl_readline(prompt)` + `__repl_readblock(prompt, cont)`（rustyline：历史/行编辑/Ctrl-D/多行 Validator 括号平衡）
- [ ] 1.3 `src/runtime/src/vm_context.rs` 加 `load_module_from_bytes`（内存字节 → live VM，返回句柄）
- [ ] 1.4 `src/runtime/src/corelib/reflection.rs` 加 `__load_bytecode_in_memory(mods)` builtin（复用 lazy loader 内核）
- [ ] 1.5 `src/runtime/src/corelib/mod.rs` 注册 3 个 builtin
- [ ] 1.6 `src/runtime/src/corelib/repl_tests.rs` 括号平衡 + 内存加载往返单测

## 阶段 2: z42.scripting 库（stdlib）
- [ ] 2.1 `z42.scripting.z42.toml`（deps 按 D2 最终裁决：A=依赖 z42c.pipeline/z42c.syntax）+ README
- [ ] 2.2 `Repl.z42`（`Std.Repl.ReadLine/ReadBlock` native 绑定）
- [ ] 2.3 `ScriptState.z42` / `EvalResult.z42`
- [ ] 2.4 `InputClassifier.z42`（6 类分类 + 括号平衡）
- [ ] 2.5 `Transcript.z42`（Growing Transcript + `$ReplVars` 昇格 + using/type 追加）
- [ ] 2.6 `Script.z42`（`Create` / `Eval`：分类→建源→PackageCompile→内存加载→Invoke→EvalResult；错误恢复）
- [ ] 2.7 `ResultFormatter.z42`（原始/对象反射/数组/null/RuntimeError）
- [ ] 2.8 `tests/eval_expr` `eval_var_persist` `eval_error_recovery` [Test]

## 阶段 3: z42.interactive REPL 宿主 + launcher（toolchain）
- [ ] 3.1 `interactive_main.z42` 改真入口：交互循环 + `-c "expr"` 单次求值
- [ ] 3.2 `ReplSession.z42`（读→分类→Eval→打印）
- [ ] 3.3 `LineEditor.z42`（封装 Std.Repl + 提示符/续行）
- [ ] 3.4 `MetaCommands.z42`（MVP 集：.help .exit .quit .reset .clear .history .save .vars .types .usings .using .mode .version）
- [ ] 3.5 `z42.interactive.z42.toml` 加依赖 z42.scripting + source 列表
- [ ] 3.6 `launcher_cli.z42` 注册 + 路由 `repl` → z42i（Z42_LIBS 三段）

## 阶段 4: 验证
- [ ] 4.1 `cargo build --release`（z42vm）无错
- [ ] 4.2 `xtask test stdlib`（含 z42.scripting）全绿
- [ ] 4.3 `xtask test compiler` 自举 gen1==gen2 逐字节不动（本 change 不碰 z42c 源）
- [ ] 4.4 `z42 repl` 手动 smoke：`1+2`、`var x=5;x*2`、编译错误保留、`.vars`/`.help`/`.exit`
- [ ] 4.5 `z42 repl -c "1+2"` → 3 后退出
- [ ] 4.6 spec scenarios 逐条覆盖确认

## 阶段 5: 文档同步（按阶段 9 触发矩阵）
- [ ] 5.1 `docs/design/toolchain/repl.md` 校正 stale（包名/路径/zpkg 数/内存加载/静态依赖决策）
- [ ] 5.2 `src/toolchain/interactive/README.md` 去 scaffold、改六段制
- [ ] 5.3 `src/libraries/z42.scripting/README.md` 六段制
- [ ] 5.4 `docs/roadmap.md` 0.4.0 REPL 状态
- [ ] 5.5 `docs/spec/changes/ACTIVE.md` 实现期登记三子系统 / 归档释放

## 备注
- D2（scripting 构建层级 A/B）、D3（内存加载 builtin A/B）、结果打印深度 —— 6.5 待 User 裁决后再动阶段 2。
- 静态依赖 PackageCompile → 不依赖 dynamic-component-registration 的接口 cast 修复。
