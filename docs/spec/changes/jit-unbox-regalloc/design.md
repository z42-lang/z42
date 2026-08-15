# Design: 现状 JIT 寄存器契约 + unbox/驻留 落地机理

> 本文档记录**当前** JIT 的 `frame.regs` 访问契约（设计所对抗的地基），以及各 sub-phase 的落地机理。
> 所有 file:line 锚点核实于 worktree `z42-jitfield` @ origin/main `6a22f26e`（2026-08-15）。

## A. 当前寄存器契约（设计约束）

**一句话**：所有 z42 寄存器状态住在 `frame.regs`（`Vec<Value>`，`frame.rs:17-19`）内存里；**没有任何值跨 op
常驻 Cranelift SSA 机器寄存器**。唯一跨块常驻 SSA 的是 prologue hoist 的**指针/长度**（`regs_base`、
hoisted array/field ptr），从不是 z42 寄存器**值**。

### A1. reg 访问 = 开放编码，无汇点
- `regs_base` 在 prologue 一次性建立：`jit_regs_ptr(frame) -> (*frame).regs.as_mut_ptr()`
  （`translate.rs:415-418`，helper `helpers/value.rs:22-25`）。指针整帧稳定（`JitFrame::new` 预分配
  `take_pooled_regs(max_reg+1)`，永不 grow，`frame.rs:41-49`）。
- 槽地址 = `regs_base + idx*VALUE_STRIDE`，**在 ~15 个 emitter 内各自开放编码**（`emit_i64_binop`
  `1884`、`emit_i64_convert` `1940`、`emit_primitive_copy` `2007`、`emit_i64_cmp` `2076`、
  `emit_const_*` `2254-2331`、array `1128/1231`、field `1366/1414`、BrCond bool `1702`…）。
  **无单一 `load_reg`/`store_reg`**。→ **Phase 2.0 的动机**。

### A2. Value 布局（各 emitter 本地重声明常量）
`VALUE_STRIDE=16`（`size_of::<Value>()`）、align 8、**tag byte @off0**、**payload @off8**
（`translate.rs:1880-1882` 等多处）。tag：`I64=0,F64=1,Bool=2,Char=3,Str=4,Null=5`（`types.rs:1592-1607`）。
pin 测试：`types_tests.rs` `value_size_observed`/`value_discriminants_pinned`/`value_*_payload_at_offset_8`
+ `types.rs` 编译期 `const _: () = assert!(size==16)`。

### A3. 翻译主循环 + 两类 op
`translate_function`（`translate.rs:279`）→ `for block` (`553`) → `for instr` (`707`) → `match instr`。
- **Category A（原生内联）**：I64 算术/比较/convert、Bool 逻辑、const、packed 数组元素、P5-B 字段——
  经 `regs_base` 直读写 `frame.regs`，每个都有 `else` helper 回落（内存表示始终 coherent）。
- **Category B（Rust helper `jit_*`）**：Div/Rem（留 /0 异常）、StrConcat/ToStr、Call/VCall/Builtin/
  ObjNew/CallIndirect、数组/对象分配、Static、Struct 系列、及一切非 typed 回落。
- **helper 传参 = 寄存器 index 非值**：`ri!` 发 i32 reg 号（`595-597`），helper `(frame,ctx,dst_idx,
  a_idx,…)` 自己按 index 读写 `frame.regs`（`jit_add` `helpers/arith.rs:16-43`）。reg 向量参数按
  `(ptr,len)` marshaling IR 的 `&[u32]`（`regs_val!` `620-632`）——**从不按值传 `Value`**。
  → **任何 SSA 缓存值对 Category-B helper 不可见，调用前必 spill 到 `frame.regs[idx]`**。

### A4. call/vcall/ctor 边界
全是 Category-B，收发都过 `frame.regs` 内存：`hr_call` `1016`、`hr_vcall` `1445`、`hr_obj_new` `1294`、
`hr_call_indirect` `1635`。callee 帧从 caller `frame.regs` 的 arg-index slice 构造
（`JitFrame::new_args_from` `frame.rs:74`）。结果 helper 写 `frame.regs[dst]`。每个 call 后
`emit_safepoint_check`（`1041`，可转 slow 扫内存）。→ **spill before + reload after + safepoint 前 coherent**。

### A5. OSR 入口
`translate_function(osr_entry: Option<usize>)`（`292`）；`Some(k)`：专用入口块跑正常 prologue，然后
`jump cl_blocks[k], &[]`（`549-550`，**空 block args**）。运行期 `try_osr`（`interp/mod.rs:604-643`）在第
`osr_threshold` 回边 `JitFrame::from_interp_regs(&frame.regs, max_reg)`（`frame.rs:59-66`，逐槽 clone）
后跳原生。→ **OSR 入口处所有 live reg 从 `frame.regs` 内存读**；2C 若让循环头带 block param，OSR 入口
必须先从内存 load 出这些 param 再 jump。

### A6. reg_types（unbox 判定基础）
`Function.reg_types: Box<[IrType]>`（`bytecode.rs:513`），从 zbc `REGT` 段加载（`zbc_reader.rs:1656`；
缺失=空 → 全回落 helper）。`IrType`（`ir_type.rs:51-72`，`#[repr(u8)]`）：`Unknown=0,I8..U64=1..8,
F32=9,F64=10,Bool=11,Char=12,Str=13,Ref=14,Void=15`。**无独立 Object/Array——堆值全塌到 `Ref`**。
- **可 unbox 标量 = I8..U64 / F32 / F64 / Bool / Char**；`Str`/`Ref` 堆。
- **关键**：`I8..U64` 运行时**全物理存 `Value::I64`**（payload i64 @off8，`emit_i64_convert` 注释
  `1926-1928` + `types.rs:1806-1819`），但快路径**只在 `== I64` 精确触发**（`is_i64_typed_unary`
  `1855`）——**Phase 2A 的 headroom**（放宽到 `is_integer()`）。

### A7. 块结构 = 无 z42-reg block param
每 z42 块一个 Cranelift 块（`cl_blocks` `310-312`）；**所有终结子 `jump`/`brif` 用空 `&[]`**（`Br`
`1675`、`BrCond` `1705`）。`append_block_params_for_function_params` 仅入口块（`321`）。→ **跨块 z42
寄存器值今天纯走 `frame.regs` 内存往返，无 phi**。块内缓存（2B）天然安全；跨块驻留（2C）须自行 threading
block param 到这些站点 + OSR 入口重建。

## B. 落地机理要点（对应 proposal 的 4 phase）

- **2.0**：`RegAccess::{load_reg,store_reg}` 汇点封装 A1 的地址算术 + A2 常量；15 站点改调汇点。纯重构。
- **2A ✅**：A6 的谓词 `== I64` → `is_integer()`（`is_int_typed`/`is_int_cmp`/`is_int_typed_unary`
  三谓词 + `Convert` 的 src 判定）。**符号性核实（校正 DRAFT）**：VM 对所有整数含 `U64` 一律
  **有符号** i64 处理比较/右移（interp `numeric_lt`/`shr` + helper `numeric_lt_helper`/
  `int_bitop_helper` 均 `x<y`/`x>>(y&63)`），故 native 沿用有符号 `icmp`/`sshr`，**不引入**
  `icmp Unsigned*`/`ushr`（否则 native 会与 helper 回落 + interp 双双背离，破 `vm-jit-consistency`）。
  真正无符号 U64 是独立 VM 级变更，不在本 change。值仍住内存，无 spill 机制。
- **2B**：块内 local value numbering，缓存 unboxed SSA 值；spill 汇点 = 块终结子前（A7）∪ Category-B
  调用前+后失效（A3/A4）∪ safepoint 前（A4）。OSR 无关（A5，块头从内存起）。
- **2C**：循环头 block param（A7 的 threading + SSA 构造）；出口 spill；**OSR 入口从 `frame.regs` load
  param 再 jump**（A5）。风险最高，建议单独 mini-DRAFT + `Z42_OSR_THRESHOLD=1` 全压测。

## C. 正确性总纲

所有 phase 的可观察行为等价靠同一不变式：**在任何「外部（helper/interp/GC/OSR/另一块）可能读 `frame.regs`」
的点，内存都 coherent**。2.0/2A 天然满足（值始终在内存）；2B/2C 靠在 §A3/A4/A5/A7 枚举的每个汇点 spill
达成。验证：`cargo --lib` + `xtask test all`（e2e interp+**jit** + stdlib + 自举 5/5 逐字节）+
`vm-jit-consistency` + `Z42_OSR_THRESHOLD=1` 压测（2C）。无格式 bump、无两代自举墙。
