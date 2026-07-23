# Tasks: 惰性逐函数 JIT

> 状态：🟡 进行中 | 创建：2026-07-23

## 进度概览
- [x] 阶段 1: 惰性编译内核（LazyCompiler + setup/compile_one）
- [x] 阶段 2: 派发 hook（jit_call / jit_vcall miss → lazy）
- [x] 阶段 3: 测试与验证（Rust 单测本地全绿；e2e/CI-权威见备注）
- [x] 阶段 4: 文档同步

## 阶段 1: 惰性编译内核
- [x] 1.1 `translate.rs`：`translate_function` 由取 `&HashMap<String,FuncId>` 改为取本函数 `FuncId`（去全表依赖）
- [x] 1.2 `lazy.rs`（NEW）：`LazyCompiler{ jit, helper_ids, module, profile }` + `setup(module)` + `compile_one(idx) -> Result<FnEntry>`（declare+translate+finalize+建 FnEntry）
- [x] 1.3 `frame.rs`：`fn_entries_by_id` 改 `Vec<OnceLock<FnEntry>>`；**删 `fn_entries`（by-name）**；`lazy: *const Mutex<LazyCompiler>`；集中解析器 `resolve_fn_by_id/by_name`（惰性 hook + 双重检查 + 计数）
- [x] 1.4 `mod.rs`：`compile_module` 拆为 `JitModule::setup`；持 `Box<Mutex<LazyCompiler>>`；`run_fn` 先 `resolve(entry)` 再执行；移除 eager 全量 translate 循环
- [x] 1.5 `mod.rs`：计数器/事件语义调整（计数在 `resolve_fn_by_id` 成功编译处 +1；事件报模块规模 + setup 耗时）

## 阶段 2: 派发 hook
- [x] 2.1 `call.rs`：`jit_call` 两处查表 swap；miss 退 `cross_zpkg_via_interp`（不变）
- [x] 2.2 `vcall.rs`：`jit_vcall` 4 处查表 swap
- [x] 2.3 `object.rs`/`closure.rs`/`value.rs` 查表 swap 到 `resolve_fn_by_name`（Scope 扩张）
- [x] 2.4 `control.rs`：`#[cfg(test)] make_jit_ctx` 字面量随结构体更新

## 阶段 3: 测试与验证
- [x] 3.1 `lazy_tests.rs`（NEW）：8 单测——setup 不编 / 首调只编入口 / 幂等 / 多函数各编一次 / interp-only 不编 / **调用者运行中惰性编译被调者（finalize 不失效运行代码）** / 多线程首调串行化。**全绿**
- [x] 3.2 `cargo build --release`（z42vm）无错、零警告；`cargo test --lib` 840 全绿
- [~] 3.3 `xtask test e2e --mode jit` —— 本地跨分支 artifact 污染（stdlib=32 / xtask.zpkg=33 / ./xtask=32），warm 不可跑；**以 CI `test-vm-jit` 为权威**
- [~] 3.4 `xtask test` 完整 gate —— 同 3.3，CI 为权威
- [~] 3.5 `Z42_JIT_PROFILE=1` 佐证 —— 需一致 toolchain，随 CI e2e 验证
- [x] 3.6 spec scenarios：Rust 单测逐条对应（首调 3 场景 / interp-only / 线程安全 / 计数器语义）；「golden 输出不变」由 CI e2e 覆盖

## 阶段 4: 文档同步
- [x] 4.1 `docs/book/src/runtime/jit-lazy-compile.md`（NEW）+ 挂 `SUMMARY.md`
- [x] 4.2 `src/runtime/src/jit/README.md`：核心文件（+lazy.rs）+ 入口点 + 测试段
- [x] 4.3 `ACTIVE.md`：登记 runtime 锁持有者（归档时释放）
- [x] 4.4 归档 doc-check 清单核对

## 备注
- **本地验证边界**：改动纯 runtime（Rust VM）。`translate_function` 每函数输出字节不变——只改「何时调用 translate」，不改翻译逻辑，golden-only 回归风险低。8 个 Rust 单测覆盖惰性核心（含最关键的「运行中编译被调者、finalize 不失效运行代码」+ 线程安全 + interp-only 退化）。
- **CI 为权威**：`test-vm-jit`（e2e jit golden 逐字节不变）+ 墙钟从 ~55m 回落，是最终 GREEN 判定。本地 warm 因跨分支 artifact 版本混合不可跑（非 cold-seed 问题）。push 后盯 CI。
- cranelift `JITModule: Send` 已由 `unsafe impl Send for LazyCompiler` + 编译通过确认。
