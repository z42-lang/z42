# Design: 并行 worker 线程跑 JIT + 编译码表全局共享

## Architecture

```
                          VmCore (Arc, 全线程共享)
                          ├── lazy_loader: Mutex<Option<LazyLoader>>
                          ├── module:      Option<Arc<Module>>
                          ├── vm_contexts: Mutex<Vec<VmContextPtr>>
                          └── jit_shared:  OnceLock<Arc<JitShared>>   ← 新增
                                            │
              ┌─────────────────────────────┴─────────────────────────────┐
              │  JitShared（编一次、全线程共享）                              │
              │  ├── fn_entries_by_id: Vec<OnceLock<FnEntry>>  (机器码指针) │
              │  ├── lazy:  Box<Mutex<LazyCompiler>>  (cranelift JITModule) │
              │  ├── module: Arc<Module>                                    │
              │  ├── merged_len / lazy_table / call_counts                  │
              │  └── jit_threshold / osr_entries / osr_threshold           │
              └─────────────────────────────────────────────────────────────┘
                    ▲ Arc::clone                    ▲ Arc::clone
        ┌───────────┴──────────┐        ┌───────────┴──────────┐
        │ entry 线程            │        │ worker 线程 i         │
        │ JitModuleCtx 薄壳     │        │ JitModuleCtx 薄壳     │
        │ { shared, vm_ctx=E }  │        │ { shared, vm_ctx=Wi } │
        └──────────────────────┘        └──────────────────────┘
              │ set_jit_ctx(&薄壳)              │ set_jit_ctx(&薄壳)
              ▼                                 ▼
        JIT 机器码 / mixed-mode          JIT 机器码 / mixed-mode
        （helper 经 (*ctx).vm_ctx 拿本线程 VmContext）
```

核心洞察：`JitModuleCtx` 现有 10 个字段里，只有 `vm_ctx: *mut VmContext` 是 per-thread；其余 9 个
「编一次就不变」。把 9 个共享字段抽进 `JitShared`（Arc），`vm_ctx` 留在 per-thread 薄壳，即得
「编一次、N 线程各带自己的 vm_ctx 跑」。

## Decisions

### Decision 1: 拆结构方式 —— 薄壳 + `Deref`，而非把 vm_ctx 挪 TLS

**问题**：`vm_ctx` per-thread、其余共享，怎么拆才既能共享码表、又不改 helper/机器码 ABI？

**选项**：
- **A（选定）**：`JitShared`（Arc）+ `JitModuleCtx { shared: Arc<JitShared>, vm_ctx }` 薄壳；薄壳
  `impl Deref<Target=JitShared>`。helper 仍收单个 `*const JitModuleCtx`；`ctx.module` / `ctx.merged_len`
  等字段读经 `Deref` 透明命中 `JitShared`；`ctx.vm_ctx` 命中薄壳自身字段。`resolve_*` 方法留在
  `impl JitModuleCtx`（它们需同时读 `self.shared.fn_entries_by_id` + `self.vm_ctx`）。
- **B（弃）**：`JitModuleCtx` 整体共享，`vm_ctx` 挪 `thread_local!`。机器码内联 safepoint 检查靠固定
  结构偏移读 `vm_ctx`（`JIT_MODULE_CTX_VM_CTX_OFFSET`）；改 TLS 后机器码要取 TLS 地址，破坏固定偏移
  优化。更差。

**决定**：A。`vm_ctx` 保持薄壳直接字段 → `offset_of!(JitModuleCtx, vm_ctx)` 照常、内联 safepoint 不动；
`Deref` 让 helper 几乎零改动。薄壳克隆廉价（Arc bump + 写 vm_ctx）。

### Decision 2: `JitShared` 拥有 `LazyCompiler` + `module: Arc<Module>`

**问题**：`lazy`/`module` 现为裸指针（指向 `JitModule` 各自拥有的 Box / caller 的 `&Module`）。跨线程
共享后生命周期怎么保证？

**决定**：
- `JitShared` 直接**拥有** `lazy: Box<Mutex<LazyCompiler>>`（不再裸指针）——cranelift 机器码页随
  `JitShared` 存活（`JitShared` 存于 `VmCore` Arc，生命周期 ≥ 所有 worker）。
- `module` 升为 `Arc<Module>`（从 `VmCore.module` clone）。跨线程共享结构里持 Arc 比裸指针更显然正确；
  且 `&*ctx.module` / `ctx.module.func_index` 经 Arc `Deref` 照旧编译，读点零改。
- `LazyCompiler`（含 cranelift `JITModule`）若非 auto-`Send` → `JitShared` 加 `unsafe impl Send + Sync`，
  安全理由：`LazyCompiler` 的一切访问都在其 `Mutex` 下串行化；机器码指针 `FnEntry` 已 `Send+Sync`。
  与现有 `unsafe impl Sync for JitModuleCtx` 同源做法。**实施首步编译期验证 auto-Send 与否**。

### Decision 3: 共享表存 `VmCore.jit_shared: OnceLock<Arc<JitShared>>`，其存在即 JIT 激活信号

**问题**：worker 怎么知道该跑 JIT 还是 interp？`Vm.default_mode` 在 entry 的 `Vm` 结构里，worker 够不着。

**选项**：
- 在 `VmCore` 存 `ExecMode`，worker 查 mode。
- **（选定）** 存 `Arc<JitShared>`——**有值即 JIT 激活**。interp entry（`crate::interp::run_with_static_init`）
  从不建 `JitShared` → worker 查 `core.jit_shared` 为空 → 自然回落 `exec_function`。一个字段同时承担
  「信号 + 数据」，无冗余状态。

**决定**：`OnceLock<Arc<JitShared>>`。`jit::run` 在 `setup` 后 `jit_shared.set(Arc::clone(&shared))`。
worker 读 `core.jit_shared.get()`。

### Decision 4: worker 执行路径 —— 抽出「在薄壳上跑一个已解析函数（带 args）」内部 runner

**问题**：`run_fn` 只跑无参 entry；worker 要跑带 `env_val` 参数的 action 函数。且静态初始化已由 entry
在 spawn 前完成，worker 不该重跑 `run()` 全套。

**决定**：从 `run_fn` 抽出 `run_resolved_on_shell(shell: &JitModuleCtx, ctx: &VmContext, entry: &FnEntry,
args: &[Value]) -> Result<()>`：建 `JitFrame::new(entry.max_reg, args)`、push VmFrame、`transmute` 调机器码、
pop/recycle。`run_fn`（entry 无参）与 worker（带 env）共用它。worker 侧新增
`run_spawned_action_jit(thread_ctx, shared, fn_name, args)`：建薄壳 → 发布 jit_ctx → `resolve_fn_by_name`
（未译则回落 interp，同 `run_fn` 现有 fallback）→ `run_resolved_on_shell`。

### Decision 5: 并发正确性 —— 本 change 最大风险，靠压力测试兜

**问题**：这套结构从未真并发跑过（历史只有 entry 单线程 JIT）。并发编译/调用是否 sound？

**分析**：
- **并发调用同一已编函数**：`FnEntry.ptr` 指向 cranelift 已 finalize 的位置无关机器码，跨线程调用安全；
  `OnceLock` 的 release/acquire 保证「看得见 ptr 的线程也看得见 finalize 后的码页」。✓
- **并发首次编译同一函数**：两线程都取 `lazy` 的 `Mutex`（串行化）+ `OnceLock` double-check（现有代码
  已做）→ 恰好编一次。✓
- **并发编译不同函数**：仍串行在 `lazy` mutex（cranelift `JITModule` 非线程安全，必须串行编译）——正确
  但编译期是串行段（Amdahl；编译占比 profile <1%，可接受）。✓
- **`lazy_table` / `osr_entries` / `call_counts`**：分别 Mutex / Mutex / AtomicU32，本就并发安全。✓

**决定**：加 `parallel_tests.rs`：N 线程共享一个 `JitShared`，混合「同函数并发首编」「异函数并发编译」
「已编函数并发狂调」三类，断言结果正确 + 无 panic/UB（`cargo test` 下 + 建议 CI 加 TSan 跑一轮）。

## Implementation Notes

- `JIT_MODULE_CTX_VM_CTX_OFFSET = offset_of!(JitModuleCtx, vm_ctx)`：薄壳布局 `{ shared: Arc(8B), vm_ctx }`
  下仍编译期算得正确偏移，内联 safepoint 机器码不动。
- helper 里 `&*(*ctx).module`：`module` 升 `Arc<Module>` 后仍 `Deref` 出 `&Module`，多数读点零改；
  仅极少数若因借用/裸指针语义报错再微调（故 3 个 helper 文件列 MODIFY 兜底）。
- `run_fn` 的 interp fallback（entry 未译）逻辑原样保留；worker 侧复用同一 fallback（未译 action → interp）。
- worker 薄壳每 spawn 建一次（Arc clone + 写 vm_ctx），随 worker 结束丢弃；`jit_ctx` 发布/清除与
  `vm_ctx` 锁步（复用现有 `set_jit_ctx(0)` 约定）。

## Testing Strategy

- **单元（并发安全，本 change 核心）**：`jit/parallel_tests.rs` —— N 线程共享 `JitShared` 三类混合压力。
- **Rust 全量**：`cargo test`（含集成）—— 确认拆结构无回归。
- **端到端 GREEN**：`xtask test` 全 stage（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）。
- **自举不动点**：z42c 自建 gen1==gen2 byte-identical —— worker 跑 JIT 后编译产物必须与串行/interp 逐字节
  一致（正确性金标准）。
- **JIT 一致性**：CI `test-vm-jit` 腿；本地 `xtask test e2e --mode jit` 抽验。
- **scaling（收益，非 gate）**：机器空闲时 `--jobs 1/4/8/16` 编 z42c.semantics 对比，或 `parallel_tests.rs`
  内建隔离微基准。满载机器上数据不可信，不作 GREEN 门。
