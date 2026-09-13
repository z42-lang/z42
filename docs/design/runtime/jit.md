# z42 JIT 后端规范

## 概述

z42 JIT 后端使用 **Cranelift** 将 z42 SSA IR 编译为原生机器码。编译在模块加载时进行（预热式 JIT），运行时直接执行原生函数指针。

---

## 架构

```
z42 Module (IR)
    │
    ▼ JitCompiler::compile_module()  ← 模块加载时一次性编译
JitModule
    ├── fn_table: HashMap<String, *const u8>  ← 原生函数指针
    ├── ctx: Arc<JitModuleCtx>                ← 运行时上下文
    └── _module: JITModule                    ← Cranelift JIT 模块（保持生命周期）
    │
    ▼ Vm::run() → JitModule::run_entry()
原生机器码执行
```

---

## 运行时上下文（JitModuleCtx）

编译后的函数需要访问以下运行时数据，打包为 `JitModuleCtx`，通过指针传入每个 JIT 函数：

```rust
pub struct JitModuleCtx {
    pub string_pool: Vec<String>,
    /// 函数名 → 原生函数指针（用于 Call 指令）
    pub fn_ptrs: HashMap<String, *const u8>,
    /// 原始 Module 引用（用于 ObjNew/VCall 的类元数据）
    pub module: *const bytecode::Module,
}
```

---

## JIT 帧（JitFrame）

每次函数调用创建一个 `JitFrame`，寄存器文件为定长 `Vec<Value>`（按 reg 编号索引，比解释器的 HashMap 快）：

```rust
pub struct JitFrame {
    pub regs: Vec<Value>,   // 大小 = max_reg + 1
    pub ret:  Option<Value>,
}
```

---

## 原生函数 ABI

每个 z42 函数编译为一个符合以下签名的原生函数：

```rust
type JitFn = unsafe extern "C" fn(
    frame: *mut JitFrame,
    ctx:   *const JitModuleCtx,
) -> u8;
// 返回值：0 = 正常返回，1 = 抛出了异常（异常值存于线程本地 PENDING_EXCEPTION）
```

调用约定：
- 调用方在调用前将参数写入 `frame.regs[0..param_count]`
- 被调方通过 `frame.ret` 返回值（如果有）

---

## 指令翻译策略

JIT 编译控制流为原生 Cranelift jump/branch，所有 Value 操作委托给 `extern "C"` helper 函数。

### 控制流（Cranelift 原生指令）

| z42 Terminator | Cranelift 指令 |
|---------------|---------------|
| `Br { label }` | `jump block_N` |
| `BrCond { cond, t, f }` | `call jit_get_bool` → `brif v, block_T, block_F` |
| `Ret { None }` | `return` (返回 0u8) |
| `Ret { Some(r) }` | `call jit_set_ret(frame, r)` → `return` |
| `Throw { reg }` | `call jit_throw(frame, reg)` → `return 1u8` |

### Helper 函数列表（extern "C"）

所有 helper 均为 `unsafe extern "C"` 函数，通过 Cranelift `call_indirect` 调用。

返回 `u8` 的 helper：0=成功，1=抛出异常（操作数类型不匹配或运行时错误）。

| 类别 | 签名 |
|------|------|
| 常量 | `jit_const_i32(frame, dst, val: i32)` |
| 常量 | `jit_const_i64(frame, dst, val: i64)` |
| 常量 | `jit_const_f64(frame, dst, val: i64/*bits*/)` |
| 常量 | `jit_const_bool(frame, dst, val: u8)` |
| 常量 | `jit_const_null(frame, dst)` |
| 常量 | `jit_const_str(frame, dst, ctx, idx: u32)` |
| 复制 | `jit_copy(frame, dst, src)` |
| 算术 | `jit_add(frame, dst, a, b) -> u8` |
| 算术 | `jit_sub / jit_mul / jit_div / jit_rem(frame, dst, a, b) -> u8` |
| 比较 | `jit_eq / jit_ne / jit_lt / jit_le / jit_gt / jit_ge(frame, dst, a, b) -> u8` |
| 逻辑 | `jit_and / jit_or(frame, dst, a, b) -> u8` |
| 逻辑 | `jit_not / jit_neg / jit_bit_not(frame, dst, src) -> u8` |
| 位运算 | `jit_bit_and / jit_bit_or / jit_bit_xor / jit_shl / jit_shr(frame, dst, a, b) -> u8` |
| 变量槽 | `jit_store(frame, var_ptr: *const u8, var_len: usize, src)` |
| 变量槽 | `jit_load(frame, dst, var_ptr: *const u8, var_len: usize)` |
| 字符串 | `jit_str_concat(frame, dst, a, b) -> u8` |
| 字符串 | `jit_to_str(frame, ctx, dst, src) -> u8`（Object 走 vtable ToString 分发）|
| 函数调用 | `jit_call(frame, ctx, dst, fn_name_ptr, fn_name_len, args_ptr, argc) -> u8` |
| 内置调用 | `jit_builtin(frame, ctx, dst, name_ptr, name_len, args_ptr, argc) -> u8` |
| 数组 | `jit_array_new / jit_array_new_lit / jit_array_get / jit_array_set / jit_array_len` |
| 对象 | `jit_obj_new / jit_field_get / jit_field_set / jit_vcall` |
| 类型检查 | `jit_is_instance / jit_as_cast` |
| 静态字段 | `jit_static_get / jit_static_set` |
| 控制辅助 | `jit_get_bool(frame, reg) -> u8`（提取 bool 用于 BrCond）|
| 控制辅助 | `jit_set_ret(frame, reg)`（写 ret 槽）|
| 控制辅助 | `jit_throw(frame, reg)`（写线程本地异常并返回 1）|

### 异常处理

延续解释器的线程本地方案：

```rust
thread_local! {
    static PENDING_EXCEPTION: RefCell<Option<Value>> = RefCell::new(None);
}
```

JIT 代码中，每个可能抛出异常的 helper 调用后检查返回值：
```
v = call jit_add(frame, dst, a, b)
brif v, exception_dispatch, next_instr
```

`exception_dispatch` 块在编译时按 exception_table 预计算：
- 若当前块在某个 try 区间内 → 跳转到对应 catch 块（调用 `jit_install_catch(frame, catch_reg)` 从线程本地取出异常值）
- 否则 → `return 1u8`（向上传播）

---

## Cranelift 依赖

```toml
[dependencies]
cranelift-jit      = "0.135"
cranelift-codegen  = "0.135"
cranelift-module   = "0.135"
cranelift-native   = "0.135"
cranelift-frontend = "0.135"
```

---

## 文件结构

```
src/runtime/src/
├── jit/
│   ├── mod.rs        # JitCompiler, JitModule — 公开 API
│   ├── frame.rs      # JitFrame, JitModuleCtx
│   ├── helpers.rs    # 所有 extern "C" helper 函数
│   └── translate.rs  # Cranelift IR 生成（每函数翻译）
```

---

## 性能模型

| 操作 | 解释器 | JIT |
|------|--------|-----|
| 控制流（跳转） | HashMap 标签查找 + Rust 循环 | 原生 jump 指令 |
| 寄存器读写 | HashMap<u32, Value> | Vec<Value> 数组索引 |
| 函数调用 | 线性扫描函数名 | 直接函数指针调用 |
| Value 运算 | 同解释器 match | 调用 helper（同等开销）|

Phase 1 JIT 的主要收益在**控制流密集**（循环、条件分支多）和**函数调用密集**的场景。

---

## 限制与后续工作

- **Phase 1**：所有 Value 操作通过 helper 调用，不做 unboxing 优化
- **Phase 2**（后续）：IR 携带类型标注后，对标量类型（i32/i64/f64/bool）生成 Cranelift 原生算术指令，消除 helper 调用开销
- **混合执行**（后续）：按函数粒度决定走 JIT 还是 Interp
