# Proposal: 惰性逐函数 JIT（lazy per-function compile-on-first-call）

## Why

当前 z42vm 在 `--mode jit` 下对**整个合并后的依赖闭包**做 eager 全量编译
（[main.rs:597-619](../../../../src/runtime/src/main.rs) 的 transitive BFS 合并 +
[jit/mod.rs:143](../../../../src/runtime/src/jit/mod.rs) `compile_module` 逐函数
translate 全体）。一个只调用少量 stdlib 函数的短命程序，也要把 z42.core 等整套 stdlib
的每个函数用 cranelift 从零编译一遍。

后果（已实测确认）：

- **main CI 红**：`test-vm-jit` 每个 golden 用例 fork 一个新进程冷编整套 stdlib，固定
  ~1:50/用例 × ~50 用例/shard → 撞 55 分钟超时，run
  [`29965013111`](https://github.com/z42-lang/z42/actions/runs/29965013111) 在 85.7
  分钟被取消（优化后基线 24 分钟）。`z42.ir` 收敛把闭包变大后越过阈值。
- 同一根因也拖慢所有短命 JIT 启动（CLI、脚本、未来的 REPL）。

**根因**：JIT「加载即全量编译」——编译了程序根本不调用的绝大多数函数。

## What Changes

- JIT 从「加载即全量编译」改为「**首次调用时逐函数编译**」（compile-on-first-call）。
- `compile_module` 拆成 **setup**（建 JITModule + 注册 helper，不翻译任何用户函数）
  + **compile_one(func)**（翻译+define+finalize 单个函数）。`jit::run` 先编入口函数，
  其余按需。
- `JitModuleCtx` 增加内部可变的惰性编译状态：**append-only 的 `fn_entries` 槽**（编译过的
  条目地址稳定，热路径读取无锁）+ 一个 `Mutex` 守护的 cranelift 编译器句柄（仅首次编译加锁）。
- `jit_call` / `jit_vcall` 的「未找到 FnEntry」分支：目标是**合并模块内、可 JIT 翻译**的函数
  → 就地 compile_one 并缓存后走 native；否则（interp-only 指令 / 真正跨包未加载）→ 维持现有
  interp fallback。**语义与覆盖不变**——被调到的函数照样编成原生码真跑，只是推迟到首次调用。
- 删除 eager 全量 translate 循环（JIT 模式统一走 lazy；AOT 未来若需 eager 另行处理）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/jit/mod.rs` | MODIFY | 拆 `compile_module` → `setup` + `run` 先编入口；移除全量 translate 循环；计数器语义调整 |
| `src/runtime/src/jit/lazy.rs` | NEW | `LazyCompiler`：持有 JITModule + helper_ids + `&Module` + append-only 条目表；`compile_one(func)` |
| `src/runtime/src/jit/frame.rs` | MODIFY | `JitModuleCtx`：`fn_entries` 改内部可变 append-only 槽 + 持 `LazyCompiler` 句柄（Mutex） |
| `src/runtime/src/jit/translate.rs` | MODIFY | 抽出「declare+translate+finalize 单函数」为可复用单元（供 compile_one 调用） |
| `src/runtime/src/jit/helpers/call.rs` | MODIFY | `jit_call` 查表改用集中式 `resolve_fn_by_id/by_name`（内含 lazy hook），miss 退 interp fallback |
| `src/runtime/src/jit/helpers/vcall.rs` | MODIFY | `jit_vcall` 4 处查表同款 swap 到集中式解析器 |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | 构造器派发查表 swap 到 `resolve_fn_by_name`（2026-07-23 实施发现：fn_entries 由此消费） |
| `src/runtime/src/jit/helpers/closure.rs` | MODIFY | CallIndirect 查表 swap（同上，语义不变） |
| `src/runtime/src/jit/helpers/value.rs` | MODIFY | ToString vcall 查表 swap（同上） |
| `src/runtime/src/jit/helpers/control.rs` | MODIFY | `#[cfg(test)] make_jit_ctx` 构造 `JitModuleCtx` 字面量随结构体变更同步 |
| `src/runtime/src/jit/lazy_tests.rs` | NEW | 单测：首调编译 / 未调不编 / 多线程首调串行化 |
| `src/runtime/src/jit/README.md` | MODIFY | 功能索引 + 核心文件 + 测试段同步 lazy 策略 |
| `docs/book/src/runtime/jit-lazy-compile.md` | NEW | 惰性编译机制页（数据结构 / 首调流程 / 线程安全 / 决策权衡，配伪代码） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 runtime 锁持有者 |

**只读引用**（理解上下文必须读，不修改）：

- `src/runtime/src/main.rs` — JIT/AOT eager 合并策略（保持不变；lazy 只改「编译时机」不改「加载/合并」）
- `src/runtime/src/vm.rs` — `ExecMode::Jit => jit::run` 入口
- `src/runtime/src/jit/mod.rs` 的 `cross_zpkg_via_interp` / `func_index` 现有 fallback 语义

## Out of Scope

- **AOT**（roadmap 仍延后；AOT 可能需要 eager 全量编译，待 AOT 落地时再处理编译策略分叉）。
- **跨进程热复用 / zygote fork**（方案 A）——为 REPL 打「stdlib 编一次跨启动复用」地基，是独立的
  未来 change，本 change 不做。
- **`test-host` 的 bench 套件膨胀（67m）与 CI 分片调整**——独立 toolchain change。
- **runner 排队争用**——基础设施层，非代码。
- **改 golden harness**——不需要：同样的 `test e2e --mode jit` 会自动变快，无 toolchain 改动。

## Open Questions

- [ ] JIT 模式彻底改 lazy、删除 eager 全量编译（推荐），还是保留一个 eager 开关？→ design Decision 1
- [ ] `jit_methods_compiled` 计数器语义由「模块函数总数」变为「实际编译数」是否可接受？→ design Decision 3
