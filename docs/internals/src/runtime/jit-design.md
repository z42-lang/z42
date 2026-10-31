# JIT 后端（Cranelift）：结构与调用约定

> 代码：`src/runtime/src/jit/`（`mod.rs` / `runtime.rs` / `frame.rs` / `lazy.rs` / `translate/` / `helpers/`）
> 相关：[惰性逐函数 JIT](jit.md)（编译时机、调用计数 tier-up、混合模式、OSR、原生快路径） · [解释器 / JIT 标量语义](interp-jit-semantics.md)
> 待办：编译在调用线程的编译锁内串行进行，尚无批量 / 后台编译（runtime-audit 待办 X-13）

本页只讲 JIT 后端的骨架：入口、两块运行时数据结构、原生函数 ABI、指令怎么翻译、异常怎么走。

## 概述

`--mode jit`（默认模式）下，z42 函数由 **Cranelift** 编成原生机器码。编译是**惰性**的：`runtime::install`
只建基础设施，不编任何用户函数；函数在调用次数达到 `jit-threshold`（默认 2）时才编译，热循环另由 OSR
在回边计数达到 `osr-threshold`（默认 10000）时转入原生码。没编译的函数、以及含 JIT 不支持指令的函数，都在解释器上跑。

## 入口

```
vm.rs: ExecMode::Jit → jit::run(ctx, module, entry)
    │
    ▼ runtime::install(ctx, module)  VmCore.jit 装上 JitRuntime（LazyCompiler 持 cranelift JITModule），
    │                                 不编译；已装则复用（每 VM 一个 JIT）
    ▼ jit::run_fn(ctx, entry)
        1. resolve_fn_by_name(ctx, entry)        —— 首次调用即编译入口
           └ 入口不可翻译 → 入口交给 interp::exec_function（其调用的已编译函数仍回跳原生码）
        2. push VmFrame（栈底帧顺带写入本线程栈下限）→ jit_fn(&mut frame, ctx) → pop
```

VM 的每条线程都跑 JIT：`__thread_spawn` 的 worker 用自己的 `VmContext`（`new_with_core`），共享同一个
`VmCore.jit`；它的解释器帧经混合模式分流进入已编译函数，到阈值就在 worker 线程上编译（编译锁内）。

## 运行时数据结构（`runtime.rs` / `frame.rs`）

**上下文参数是运行线程自己的 `VmContext`**：每个原生函数和每个 helper 都收 `ctx: *const VmContext`——调用它的那条
线程的上下文。可变执行状态（帧栈、待抛异常、寄存器池、arena、计数器分片、safepoint 计数、栈下限）因此都直接是本线程的。
生成码按偏移直接读三个字段：

| 字段（`VmContext` 上） | 偏移常量 | 作用 |
|---|---|---|
| `jit_head.stack_limit` | `VM_CONTEXT_STACK_LIMIT_OFFSET` | 序言栈深检查的下限：驱动本 ctx 的线程的栈下限，栈底帧 push 时写入 |
| `jit_head.cctors` | `VM_CONTEXT_CCTORS_OFFSET` | VM 的 `CctorRegistry`；内联字符串原语读它的包初始化门计数 |
| `safepoint_skip` | `VM_CONTEXT_SAFEPOINT_SKIP_OFFSET` | safepoint 节流计数，内联快路 load/store |

`jit_head` 是 `#[repr(C)]` 的 `JitHead`（`vm_context/types.rs`），另缓存一份 `&VmCore.jit`（`VmContext::jit()` 一次 load
取到共享运行时；早于安装的 ctx 在下一次栈底帧 push 时补上）。

**`JitRuntime`**（`VmCore.jit: OnceLock<JitRuntime>`）：每 VM 一份、所有线程共享，随 VmCore 释放，helper 经
`ctx.jit()` 取（`helpers::jit_rt`）。主要字段：

| 字段 | 作用 |
|---|---|
| `slots: SparseSegTable<JitSlot>` | 每函数的 JIT 状态，按 JIT id（= `FnId`：入口函数 `0..n`、惰性函数其后）索引：`entry`（已编译）、`count`（tier-up 计数）、`state`（Rejected 负缓存）；读无锁 |
| `module` | 指回字节码 `Module`（类描述、函数体、`func_index`）——JIT id 相对它 |
| `funcs` | VM 的 `FuncTable`，惰性函数按 `FnId` 取 |
| `lazy` | `Mutex<LazyCompiler>`：编译在调用线程上串行进行，热路径只读 `OnceLock` |
| `jit_threshold` | tier-up 阈值（计数在 `JitSlot.count`，各线程一起计） |
| `osr_entries` / `osr_threshold` | OSR 入口缓存（按「JIT id, 循环头块」，入口与惰性函数都支持）与阈值 |

跨线程的安全性（编译锁不 park 的理由、代码发布、调用点缓存、GC 扫描）见 [jit.md「线程安全」](jit.md#线程安全)。

**`JitFrame`**：一次调用一份，`#[repr(C)]`——生成码按 `offset_of!` 导出的偏移直接读写其中三个字段。

| 字段 | 作用 |
|---|---|
| `regs_ptr: *mut Value` | 首字段（偏移 0），即 `regs.as_mut_ptr()`。函数序言 load 一次作为 `regs_base`，之后所有内联寄存器访问都按 `regs_base + idx * 16` 算地址 |
| `ret: Value` | 返回值。`Ret %r` 把寄存器槽的 16 B 原样拷进来（`Value` 是 `Copy`，无引用计数、无写屏障）；寄存器驻留在 SSA 里时直接写 tag + payload |
| `has_ret: u8` | `Ret %r` 置 1；无值的 `Ret` 不碰它，保持 0。`enter_compiled` 据此给出 `Returned(Some(ret))` 或 `Returned(None)`，void 与返回 `null` 由此区分 |
| `regs: Vec<Value>` | 寄存器文件，按 SSA 寄存器号索引，长度 `max_reg + 1` |
| `frame_id` | 帧 id，供 struct 值的悬垂检查；OSR 时继承 interp 帧的 id |

`regs_ptr` 能一直有效，靠的是寄存器文件在帧存活期间从不改变长度：构造时从寄存器池取定长 `max_reg + 1`，
生成码与 helper 只按下标读写；唯一会给别的帧 `resize` 的 `store_thru_ref` 碰不到 JIT 帧（`LoadLocalAddr`
不进 JIT，且槽号总 `<= max_reg`）。`JitFrame` 本身被 move 不影响堆上的缓冲区。`ret` 不是 GC 根：
从写入到 `enter_compiled` 取走之间没有 safepoint。

每次进入原生函数都经 `invoke::enter_compiled`：把 `FnEntry.func` 与 `regs` 登记成一个 `VmFrame` 压进 `VmContext` 的调用栈（GC 从那里扫描根，栈回溯从 `func` 现算名字与行号），运行后弹出并回收 `JitFrame`。调用类 helper 与 `jit_throw` 只收位点的代码偏移（`linear_offset` 常量），戳到调用方帧的 `pc` 上。

## 原生函数 ABI

```rust
// helpers/mod.rs
pub type JitFn = unsafe extern "C" fn(frame: *mut JitFrame, ctx: *const VmContext) -> u8; // ctx = 运行线程的上下文
// 0 = 正常返回（frame.has_ret != 0 时返回值在 frame.ret）；1 = 抛出异常（异常值挂在 VmContext 上）
```

调用方先把实参写进被调方 `frame.regs[0..argc]`。z42 函数之间**从不**生成 Cranelift 直接调用，
一律经 `jit_call` / `jit_vcall` / `jit_call_indirect` 等 helper 按 id 或站点 IC 解析目标，所以每个函数都能独立编译。

## 指令翻译（`translate/`）

- 每个 z42 基本块对应一个 Cranelift 块；`Br` / `BrCond` / `Ret` / `Throw` 译成原生跳转与返回。
  `Ret %r` 内联把返回值写进 `frame.ret` 并置 `frame.has_ret = 1`（偏移见 `JIT_FRAME_RET_OFFSET` / `JIT_FRAME_HAS_RET_OFFSET`），不调 helper。
  `BrCond` 先调 `jit_get_bool`（返回 0 / 1，非 Bool 时返回 `JIT_GET_BOOL_ERR` 并挂异常）。
- 回边（后向 `Br`，以及有一个目标在后方的 `BrCond`）前内联 safepoint 快路（两次 load/store + 分支），慢路调 `jit_check_safepoint_slow`；函数序言内联栈深检查，越界调 `jit_stack_overflow`。
- 类型已知的整数 / 浮点标量运算、比较、转换、除余，以及部分字段读写，直接生成原生指令（寄存器缓存、loop-carried 驻留等见 [jit.md](jit.md)）。
- 其余操作调 `extern "C"` helper。helper 在 `helpers/registry.rs` 统一登记：`register_symbols` 把符号交给 `JITBuilder`，
  `declare_imports` 在模块里声明导入，生成码用普通 `call` 调用。约定：前两个参数总是 `(frame, ctx)`；
  可能失败的返回 `u8`（0 成功，1 异常），不会失败的返回 `()`。
- 不可翻译的指令集中在 `translate/unsupported.rs`（`CallNative`、`PinPtr`、`LoadLocalAddr` 等地址类指令、
  方法级泛型**函数体**读型参的 `MethodTypeArg` / `MethodDefault`、方法级形参的 `new T[n]`、带 `$mta:N` 转发标记的
  泛型调用等）。含这些指令的函数在编译前就被拒绝，留在解释器上。
- **方法级泛型的调用站点可以编译**：站点的 `method_type_args` 是指令里的编译期常量，翻译时把它的 `(ptr, len)`
  烘进机器码交给 `jit_call_mta` / `jit_vcall_mta`；callee 已编译 ⇒ 直接 native 且**不传**型参，否则解释执行并线程
  进 callee 帧。这依赖 `unsupported.rs` 头注的不变式——「能编译 ⇔ 函数体观察不到 `frame.method_type_args`」：
  运行期读它的指令全在拒译表里，所以已编译的 callee 拿到也不会看。解释器侧对称：泛型调用的 callee 一旦编译也走
  mixed-mode 转接。（动机：z42c 四个遍的 visitor 入口各含一处 `e.Accept<R, A>(this, arg)`，此前整函数留解释器。）
  类级形参的 `new T[n]` 可以编译：实参在 reg 0 接收者的 `type_args` 上，翻译时把形参下标传给 `jit_array_new`，
  由 `objops::array::class_type_param_elem` 解析出基元 T 的打包宽度与零值（M13：数组按该宽度打包；与解释器 `array_new` 同一实现）——
  否则 `Dictionary.Grow` 这类含 `new TKey[n]` 的扩容循环整段留在解释器里。

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
├── mod.rs           jit::run / run_fn
├── runtime.rs       JitRuntime（VmCore.jit）：JitSlot 表、fn_of / id_of_func / id_by_name、resolve_fn_by_* / OSR 入口解析、install
├── lazy.rs          LazyCompiler：持 cranelift JITModule，compile_fn / compile_fn_osr 按需编译单函数
├── frame.rs         JitFrame、FnEntry
├── reg_access.rs    frame.regs 槽位读写的唯一出口
├── vm_interface.rs  编译期读取 VM 元数据的只读接口
├── translate/       z42 指令 → Cranelift IR（按指令类别拆分；unsupported.rs 为不可翻译表）
└── helpers/         extern "C" helper（按指令类别拆分；registry.rs 为中央注册表）
```

加 helper 的步骤见 `src/runtime/src/jit/README.md` 的「Helper 边界」。
