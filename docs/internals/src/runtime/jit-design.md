# JIT 后端（Cranelift）：结构与调用约定

> 代码：`src/runtime/src/jit/`（`mod.rs` / `frame.rs` / `lazy.rs` / `translate/` / `helpers/`）
> 相关：[惰性逐函数 JIT](jit.md)（编译时机、调用计数 tier-up、混合模式、OSR、原生快路径） · [解释器 / JIT 标量语义](interp-jit-semantics.md)
> 待办：只有执行入口的线程跑 JIT，VM 创建的其他线程全程解释执行

本页只讲 JIT 后端的骨架：入口、两块运行时数据结构、原生函数 ABI、指令怎么翻译、异常怎么走。

## 概述

`--mode jit`（默认模式）下，z42 函数由 **Cranelift** 编成原生机器码。编译是**惰性**的：`JitModule::setup`
只建基础设施，不编任何用户函数；函数在调用次数达到 `jit-threshold`（默认 2）时才编译，热循环另由 OSR
在回边计数达到 `osr-threshold`（默认 10000）时转入原生码。没编译的函数、以及含 JIT 不支持指令的函数，都在解释器上跑。

## 入口

```
vm.rs: ExecMode::Jit → jit::run(ctx, module, entry)
    │
    ▼ JitModule::setup(module)      建 LazyCompiler（持 cranelift JITModule）+ JitModuleCtx，不编译
    ▼ JitModule::run_fn(ctx, entry)
        1. JitModuleCtx.vm_ctx ← ctx；stack_limit ← 本线程栈下限
        2. ctx.set_jit_ctx(&JitModuleCtx)        —— 让本次运行里的 interp 帧能回跳原生码
        3. resolve_fn_by_name(entry)             —— 首次调用即编译入口
           └ 入口不可翻译 → 整个入口交给 interp::exec_function
        4. push VmFrame → jit_fn(&mut frame, &ctx) → pop
        5. 清空 vm_ctx / jit_ctx
```

JIT 只在执行 `run_fn` 的那个线程上运行。VM 创建的其他线程用各自的 `VmContext`，其 `jit_ctx` 恒为 0，全程解释执行。

## 运行时数据结构（`frame.rs`）

**`JitModuleCtx`**：每个 `JitModule` 一份，以 `*const JitModuleCtx` 传给每个原生函数和每个 helper。主要字段：

| 字段 | 作用 |
|---|---|
| `fn_entries_by_id: Vec<OnceLock<FnEntry>>` | 已编译函数表，下标 = 合并模块里的函数 id；空槽 = 未编译或不可翻译 |
| `lazy_table` | 惰性加载（尚未合并进模块）的函数的编译槽，id ≥ `merged_len` |
| `module` | 指回字节码 `Module`（类描述、函数体、`func_index`） |
| `lazy` | `Mutex<LazyCompiler>`：首编串行化，热路径只读 `OnceLock` |
| `vm_ctx` | 本次运行的 `VmContext`；helper 经它访问可变 VM 状态 |
| `call_counts` / `jit_threshold` | 调用计数 tier-up |
| `osr_entries` / `osr_threshold` | OSR 入口缓存（按「函数 id, 循环头块」）与阈值 |
| `stack_limit` | 函数序言做栈深检查用的下限 |

`vm_ctx` 和 `stack_limit` 的偏移由 `offset_of!` 导出给生成码，内联 safepoint 检查和栈检查直接 load。

**`JitFrame`**：一次调用一份。

| 字段 | 作用 |
|---|---|
| `regs: Vec<Value>` | 寄存器文件，按 SSA 寄存器号索引，长度 `max_reg + 1` |
| `ret: Option<Value>` | 返回值，由 `jit_set_ret` 写入 |
| `env_arena` | 不逃逸闭包的帧内环境 |
| `frame_id` | 帧 id，供 struct 值的悬垂检查；OSR 时继承 interp 帧的 id |

每次进入原生函数都经 `invoke::call_native`：把 `regs` / `env_arena` 登记成一个 `VmFrame` 压进 `VmContext` 的调用栈（GC 从那里扫描根），运行后弹出并回收 `JitFrame`。

## 原生函数 ABI

```rust
// helpers/mod.rs
pub type JitFn = unsafe extern "C" fn(frame: *mut JitFrame, ctx: *const JitModuleCtx) -> u8;
// 0 = 正常返回（返回值在 frame.ret）；1 = 抛出异常（异常值挂在 VmContext 上）
```

调用方先把实参写进被调方 `frame.regs[0..argc]`。z42 函数之间**从不**生成 Cranelift 直接调用，
一律经 `jit_call` / `jit_vcall` / `jit_call_indirect` 等 helper 按 id 或站点 IC 解析目标，所以每个函数都能独立编译。

## 指令翻译（`translate/`）

- 每个 z42 基本块对应一个 Cranelift 块；`Br` / `BrCond` / `Ret` / `Throw` 译成原生跳转与返回。
  `BrCond` 先调 `jit_get_bool`（返回 0 / 1，非 Bool 时返回 `JIT_GET_BOOL_ERR` 并挂异常）。
- 回边和 `BrCond` 前内联 safepoint 快路（两次 load/store + 分支），慢路调 `jit_check_safepoint_slow`；函数序言内联栈深检查，越界调 `jit_stack_overflow`。
- 类型已知的整数 / 浮点标量运算、比较、转换、除余，以及部分字段读写，直接生成原生指令（寄存器缓存、loop-carried 驻留等见 [jit.md](jit.md)）。
- 其余操作调 `extern "C"` helper。helper 在 `helpers/registry.rs` 统一登记：`register_symbols` 把符号交给 `JITBuilder`，
  `declare_imports` 在模块里声明导入，生成码用普通 `call` 调用。约定：前两个参数总是 `(frame, ctx)`；
  可能失败的返回 `u8`（0 成功，1 异常），不会失败的返回 `()`。
- 不可翻译的指令集中在 `translate/unsupported.rs`（`CallNative`、`PinPtr`、`LoadLocalAddr` 等地址类指令、
  方法级泛型的 `MethodTypeArg` 与泛型调用等）。含这些指令的函数在编译前就被拒绝，留在解释器上。

## 异常

异常值存在 `VmContext` 上（helper 用 `set_exception` 写入），不放线程本地变量。每个可能失败的 helper 调用后都检查返回值：

```
v = call jit_xxx(frame, ctx, …)
brif v, exc_block, next
```

`exc_block` 在编译期按异常表生成：

- 当前块不在任何 try 区间内 → `return 1`，向调用方传播；
- 在 try 区间内 → 先调 `jit_fatal_pending`（栈溢出等致命错误跳过所有 handler，直接传播），
  再按 catch 条目依次 `jit_match_catch_type` 比对类型，命中的调 `jit_install_catch` 取出异常值写进 catch 寄存器后跳转；
  通配 catch 直接进入；都不匹配 → `return 1`。

## 文件结构

```
src/runtime/src/jit/
├── mod.rs           JitModule（setup / run_fn）、jit::run
├── lazy.rs          LazyCompiler：持 cranelift JITModule，compile_one 按需编译单函数
├── frame.rs         JitFrame、JitModuleCtx、FnEntry、resolve_fn_by_* / OSR 入口解析
├── reg_access.rs    frame.regs 槽位读写的唯一出口
├── vm_interface.rs  编译期读取 VM 元数据的只读接口
├── translate/       z42 指令 → Cranelift IR（按指令类别拆分；unsupported.rs 为不可翻译表）
└── helpers/         extern "C" helper（按指令类别拆分；registry.rs 为中央注册表）
```

加 helper 的步骤见 `src/runtime/src/jit/README.md` 的「Helper 边界」。
