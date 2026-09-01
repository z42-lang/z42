# Tasks: 并行 worker 线程跑 JIT + 编译码表全局共享

> 状态：🟡 进行中（实现 + 本地 cargo 全绿；完整 xtask GREEN + 自举不动点交 CI） | 创建：2026-09-01

## 进度概览
- [x] 阶段 1: 拆 `JitModuleCtx` → `JitShared` + 薄壳
- [x] 阶段 2: 共享表上浮 `VmCore` + entry 接线
- [x] 阶段 3: worker JIT 执行路径
- [x] 阶段 4: 并发正确性测试
- [x] 阶段 5: 验证与文档同步（CI 门项除外）

## 阶段 1: 拆结构（`jit/frame.rs` + `jit/mod.rs`）
- [x] 1.1 `LazyCompiler` 已 `unsafe impl Send`（非 `Sync`）→ 拥有 `Box<Mutex<LazyCompiler>>` + `*const Module` 后 `JitShared` 加 `unsafe impl Send+Sync`（沿用现有做法）
- [x] 1.2 `frame.rs`：定义 `JitShared`（9 个共享字段 + 拥有 `Box<Mutex<LazyCompiler>>`；`module` 保留 `*const Module` 裸指针，减少 churn）
- [x] 1.3 `frame.rs`：`JitModuleCtx` 改薄壳 `{ shared: Arc<JitShared>, vm_ctx }` + `impl Deref<Target=JitShared>`
- [x] 1.4 `frame.rs`：`resolve_*` 方法留 `impl JitModuleCtx`；字段读经 `Deref` 自动命中共享字段（方法体几乎零改）
- [x] 1.5 `frame.rs`：`JitShared` 的 `unsafe impl Send+Sync` + 安全注释；`JIT_MODULE_CTX_VM_CTX_OFFSET` 仍正确（薄壳 `vm_ctx` 直接字段）
- [x] 1.6 `mod.rs`：`JitModule` 持 `ctx: Box<JitModuleCtx>`（`ctx.shared` 保活 `JitShared`）；`setup` 建 `JitShared`；删 `_lazy` 字段
- [x] 1.7 `mod.rs`：抽出 free `run_fn_on_shell(shell, ctx, name, args) -> ExecOutcome`，`run_fn`（entry 无参）复用
- [x] 1.8 `cargo check`（z42vm）通过；`helpers/{call,object,value}.rs` 经 Deref 零改（降为只读引用，见 proposal Scope 修正）

## 阶段 2: 共享表上浮 VmCore（`vm_context/types.rs` + `construct.rs` + `jit/mod.rs`）
- [x] 2.1 `types.rs`：`VmCore` 加 `#[cfg(feature="jit")] jit_shared: OnceLock<Arc<JitShared>>`；`construct.rs` 初始化为空
- [x] 2.2 `mod.rs`：`jit::run` 在 `setup` 后 `ctx.core.jit_shared.set(Arc::clone(&jit_module.ctx.shared))`
- [x] 2.3 interp entry（`run_with_static_init`）不建 `JitShared` → `jit_shared` 保持空（worker 自然回落 interp）

## 阶段 3: worker JIT 执行（`corelib/threading.rs`）
- [x] 3.1 `run_spawned_action`：`#[cfg(feature="jit")]` 查 `thread_ctx.core.jit_shared.get()`
- [x] 3.2 有值 → 建薄壳 `JitModuleCtx { shared: Arc::clone, vm_ctx: null }` → `crate::jit::run_fn_on_shell`
- [x] 3.3 `run_fn_on_shell` 内含 `resolve_fn_by_name`（未译回落 interp）+ 带 env args；返回 `ExecOutcome`
- [x] 3.4 无值 / 无 jit feature → `interp_action_outcome`（抽出的 interp 路径）
- [x] 3.5 jit_ctx 发布/清除与 vm_ctx 锁步（`run_fn_on_shell` 每条退出路径清除）；worker 与 entry 各自格式化异常（`Std.ThreadException` 语义不变）

## 阶段 4: 并发正确性测试（`jit/parallel_tests.rs`）
- [x] 4.1 `concurrent_same_fn_compiles_once`：16 线程并发首编同一函数 → 只编一次 + 全 Ok
- [x] 4.2 `concurrent_distinct_fns_do_not_corrupt_tables`：16 线程各编不同函数 → 全成功、槽全填
- [x] 4.3 `concurrent_callers_compile_shared_callee_once`：16 线程经 `jit_call` 并发编共享 callee
- [x] 4.4 `mod.rs` 挂 `#[cfg(test)] #[path="parallel_tests.rs"] mod parallel_tests;`
- [x] 4.5 12× release 复跑无 flake（后台任务确认，0 fail/panic）

## 阶段 5: 验证与文档
- [x] 5.1 `cargo build --release`（z42vm）无错、无新警告（顺带删孤儿 `take_exception_error`）
- [x] 5.2 `cargo test --lib`（全量 1029 passed / 0 failed，含新 parallel_tests 3 个）
- [ ] 5.3 `xtask test` 全 stage 全绿（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）—— **本机 xtask 挂 → 交 CI**
- [ ] 5.4 自举 gen1==gen2 byte-identical（worker 跑 JIT 后产物一致）—— **交 CI**
- [x] 5.5 spec 场景覆盖：worker 跑 JIT（`jit_methods_compiled` 12 vs baseline 1 坐实）+ interp/jit/baseline `total` 逐字节一致 + 并发编一次（4.1–4.3）
- [x] 5.6 `jit/README.md` 同步（核心文件登记 `JitShared`/薄壳/`run_fn_on_shell` + 测试命令 + parallel_tests）
- [x] 5.7 `docs/book/src/runtime/jit-lazy-compile.md` 加「并行 worker 跑 JIT + 共享码表」机制节 + 对齐日期
- [ ] 5.8 归档（tasks 改 🟢 + mv archive + doc-check 清单）随本分支进同一 PR —— **CI 全绿后**

## 备注
- **实测改进**（8 线程算术 workload，同机 A/B）：worker interp 1.18s → worker JIT 0.20s ≈ **5.84×**；`jit_methods_compiled` 1→12 坐实 worker 上了 JIT。
- scaling 上限受 workload 性质影响；z42c `--jobs N` 自建改进 < 5.8×（Amdahl + alloc + lazy_loader 锁），但方向由「越多越慢」翻正——根因已解。
- arc-swap lazy_loader / JIT is-check memo 明确 Out of Scope，留后续 change。
