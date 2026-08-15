# Proposal: JIT 标量 unbox + 机器寄存器驻留（打破 JIT≈interp 天花板）

> 状态：**DRAFT**（规范先行，待 User 确认 scope/approach 后分 PR IMPL）。
> 性质：vm 类变更（纯 runtime JIT codegen，无格式 bump、self-host 字节不动）。
> 前置地基：对象字节布局统一（`object-abi.md`）+ P5-B 字段原生访问（`jit-lazy-compile.md` §P5-B）。

## Why

统一对象布局把 JIT 寄存器文件压到 16B tagged `Value`、P5-B 又让对象原语字段走原生字节访问，
但 **JIT 相对 interp 仍只有 ~1.1–1.3×**（[[vm-perf-levers-and-jit-ceiling]] 已核实）。根因是**架构性**的：

> **所有 z42 寄存器状态都住在 `frame.regs`（`Vec<Value>`）内存里，没有任何值跨 op 常驻 Cranelift SSA
> 机器寄存器。** 连最快的 i64 加法（`translate.rs:1874 emit_i64_binop`）也是
> `load payload@off8 → iadd → store tag+payload`——每个 op 一次内存往返。interp 也这样。JIT 只省掉了
> **解释器 opcode dispatch**（前测 ~3%），**没拿到经典 JIT 的两大收益：热值常驻机器寄存器 + unboxed 算术。**

具体两个可量化的浪费（均由架构地图核实，见 `design.md`）：

1. **窄整数全走 helper**：`emit_i64_binop`/`cmp`/`convert` 的快路径**只在 `reg_types[reg] == IrType::I64`
   精确匹配时触发**（`is_i64_typed_unary` `translate.rs:1855`）。但 `I8..U64` 在运行时**全部物理存为
   `Value::I64`（payload i64 @off8，`types.rs:1806`）**——一个 `i32 + i32` 今天却路由到 `jit_add`
   Rust helper（native→Rust call + `RefCell` borrow + tag 分派），纯粹因为谓词没放宽。**几乎免费的一块。**

2. **热标量每 op 内存往返**：`for (…) s = s + this.v` 里 `s` 每次迭代 `load frame.regs[s] → iadd →
   store frame.regs[s]`。若 `s` 能常驻机器寄存器、只在必要边界 spill 回内存，热循环内的 load/store 流量可近乎消除
   ——这正是「真 native」与「省 dispatch」的分界。

本 change 是 [[vm-perf-levers-and-jit-ceiling]] **杠杆 B** 的落地：把 JIT 从「省 dispatch」推进到「热区标量真 native」。

## What Changes（分 4 个递进 sub-phase，各自独立 PR）

> 递进顺序按「风险/收益」排：先做零风险的地基与窄整数放宽（立刻可量化收益、de-risk），
> 再做块内 unbox，最后做真正打破天花板但最难的 loop-carried 跨块驻留。**每 sub-phase 一个 PR、
> 各自 benchmark 驱动、独立可回退**；后一 phase 未做不影响前一 phase 的正确性与收益。

### Phase 2.0 — 集中 reg 访问 choke point（纯重构，行为不变）

**问题**：`regs_base + idx*16` 的地址算术在 ~15 个 emitter 里**开放编码**（`emit_i64_binop`/`cmp`/
`convert`/`const`/array/field/…，`VALUE_STRIDE`/`PAYLOAD_OFFSET` 各自本地 `const` 重声明）——**没有单一
`load_reg`/`store_reg` 汇点**。任何寄存器缓存都需要一个统一 hook 点。

**做**：引入 `RegAccess`（或 `RegFile` 视图）——`load_reg(builder, reg) -> ClifValue`（含 tag+payload
读取）与 `store_reg(builder, reg, tag, payload)`，替换全部开放编码站点；`STRIDE`/`OFFSET`/`TAG_*` 常量
集中到一处（同时消除 §2 的常量重复）。**纯重构**：输出字节应与改前逐字等价（或仅 CLIF 结构等价、
最终机器码不变）；self-host 5/5、e2e interp+jit byte-identical 不动。**这是 2B/2C 的必要地基**（缓存
只需改 `RegAccess` 一处，而非 15 处）。

### Phase 2A — 放宽整数原生快路径到全宽度 I8..U64（低风险、立即收益）✅ 已实现

**做**：把 `is_int_typed`（原 `is_i64_typed`）/`is_int_cmp`/`is_int_typed_unary` 三个触发谓词，以及
`Convert` 的 `src` 谓词，从「`== IrType::I64`」放宽到「`IrType::is_integer()`（I8..U64）」。合法性：这些
整数**运行时全是 `Value::I64` 物理表示**（payload i64 @off8），算术按 i64 wrapping 语义计算后存
`Value::I64`——与 helper 的 I64 fast-path（`jit_add` 等）和 interp 的 `vm-wrapping-int-arith`
逐字节一致。窄化由**已有的** `Convert`（`emit_i64_convert`）在需要时插入，不是本 phase 的事（z42 无隐式
窄化 → 中间值恒 i64、只在显式 convert 处收窄）。

**⚠️ 符号性（DRAFT 原假设的事实校正，2026-08-15 实现时核实）**：DRAFT 原写「`U64` 比较用
`icmp Unsigned*`、右移用 `ushr`」。**核实源码后否决**——当前 z42 VM（interp `ops::numeric_lt`
+ `exec_value::shr` **与** JIT helper `numeric_lt_helper`/`int_bitop_helper`）对**所有**整数类型
（含 `U64`）**一律按有符号 i64** 处理比较与右移（`x < y`、`x >> (y & 63)` 算术移位）。若 native 路径
改用无符号指令，就会与 helper 回落路径**和** interp **双双背离** → 破坏 `vm-jit-consistency` 逐字节
门禁。故 native 路径**刻意沿用有符号** `icmp`/`sshr`，与 VM 现状对齐。**把 `U64` 做成真正无符号**
是一次独立的、需同时改 interp + helper + JIT 的 VM 级语义变更，**不在本 change 范围**（见 Out of Scope）。

**F32** 不在此 phase（仍回落或延后到 2B 的 float 臂）。

**收益（实测）**：窄整数密集代码从 helper 转 native——`scratch_bench/p2a/narrowint.z42`（sbyte/short/
int/long/byte/ushort/uint/ulong 混合、含 U64 高位比较/移位、Convert/Neg/BitNot）interp==jit 逐字节
一致（`8331491328502177984`），JIT 对该 workload **1.5× 快于 interp**。无需任何 spill 机制、值仍住
内存——**是 P5-B 之后又一块「免费」native 化**。

### Phase 2B — 块内标量 unbox + 机器寄存器缓存（block-local）

**做**：在**单个基本块内**，把标量寄存器（I8..U64/F32/F64/Bool/Char——`reg_types` 判定的非堆值）的
值缓存在 Cranelift SSA 值里，块内后续 op 直接用 SSA 值、不再 `load frame.regs`。在下列**汇点必须 spill
回 `frame.regs[idx]`**（架构地图 §3/§4/§5 枚举）：

- **块终结子前**（`Br`/`BrCond`/`Return`/`Throw`——跨块值仍走内存，本 phase 不引入 block param）；
- **每个 Category-B helper / Call / VCall / CallIndirect / ObjNew / Builtin 前**（helper 按 index 读
  `frame.regs`，SSA 缓存对它不可见）+ **调用后**该 dst/arg 槽视为失效、重新 load；
- **每个 safepoint 前**（`emit_safepoint_check`，可能转 slow helper 扫内存）。

**正确性**：块内缓存 = 标准 local value numbering；因每个块出口/调用/safepoint 都 spill，`frame.regs`
在任何「可能被外部观察」的点都 coherent → 与改前逐字等价的可观察行为。**OSR 无关**（OSR 只在块入口
`cl_blocks[k]` 进入，块内缓存从该块头的 load 建立，天然从内存起步）。

**收益范围（诚实标注）**：块内**多次触及同一值**或**算术链**（`t=a+b; u=t*c; …` 同块）省掉中间往返。
**单次触及的 loop-carried 标量（如每迭代一次 `s+=…`）本 phase 收益有限**——那需要 2C。

### Phase 2C — loop-carried 跨块寄存器驻留（真正的天花板突破）

**做**：为**循环携带的热标量**在**循环头块引入 Cranelift block param**（IR reg → block param 的
SSA 构造 / 支配边界 phi），使 `s`、`i` 等**跨迭代常驻机器寄存器**，循环体内零 `frame.regs` 往返。
需要：

- 循环头及所有前驱边（`Br`/`BrCond` 的 `translate.rs:1675/1705` 站点）threading block param；
- **所有循环出口** spill 回 `frame.regs`（供 helper/Return/后续块读）；
- **OSR 入口重载**：OSR 跳 `cl_blocks[k]`、当前用空 `&[]` block args（`translate.rs:549`）——若循环头
  变成带 param 的块，OSR 入口必须先从 `frame.regs` load 出这些 param 再 jump（架构地图 §5：OSR 处所有
  live reg 从内存读，寄存器驻留假设在此失效）；
- Category-B 调用/safepoint 处同 2B 的 spill/reload。

**这是打破 `s+=…` 循环 JIT≈interp 的唯一路径**，也是**风险最高**的一块（SSA 构造 + 全出口/OSR 正确性）。
**建议 2C 单独再走一次 mini-DRAFT + 强化验证**（`Z42_OSR_THRESHOLD=1` 全压测 byte-identical + 每类
循环形态覆盖），视 2A/2B 实测收益再决定投入。

## Scope（允许改动的文件，按 phase）

| 文件 | phase | 变更 | 说明 |
|------|-------|------|------|
| `src/runtime/src/jit/translate.rs` | 2.0/2A/2B/2C | MODIFY | reg 访问汇点重构 + 谓词放宽 + 块内缓存 + block-param threading |
| `src/runtime/src/jit/reg_access.rs`（或 `frame.rs` 内） | 2.0 | NEW | `RegAccess` load/store 汇点 + 集中常量 |
| `src/runtime/src/jit/frame.rs` | 2C | MODIFY | OSR 入口 reload live block-param regs |
| `src/runtime/src/jit/helpers/*.rs` | — | 只读参考 | Category-B 边界不变（本 change 只改「调用前 spill」侧，不改 helper 签名/ABI）|
| `src/runtime/src/metadata/types_tests.rs` | 2.0 | MODIFY | 若常量集中，pin 测试引用点跟随 |
| `src/runtime/src/jit/mod.rs` / `bench` | 2A+ | NEW | 把「非可提升窄整数/标量热循环」收进 criterion/scenario 锁收益 |
| `docs/book/src/runtime/jit-lazy-compile.md` | 每 phase | MODIFY | 新增「标量 unbox / 寄存器驻留」机制节（数据结构、spill 汇点表、OSR 重载、mermaid）|

**只读引用**：`docs/design/runtime/ir.md`、`metadata/ir_type.rs`、`metadata/bytecode.rs`（REGT 加载）。

## Out of Scope（本 change 不做）

- **`Str`/`Ref` 堆值的寄存器驻留**：引用值 spill 涉及 GC 可见性/write barrier，超出「标量 unbox」范围；
  P5-B 已把引用字段留 helper，这里同样只碰 `reg_types` 判定的**非堆标量**（I8..U64/F32/F64/Bool/Char）。
- **完整 SSA 寄存器分配器 / 线性扫描 regalloc**：本 change 依赖 Cranelift 自身的寄存器分配（我们只决定
  「哪些 IR reg 提升为 SSA 值 + 何处 spill」，机器寄存器分配交给 Cranelift）。不自造 regalloc。
- **div/rem、StrConcat、数组/对象分配、convert-to-float 的 helper inline**：这些 helper 边界的 native
  化是独立正交增量（[[vm-perf-levers-and-jit-ceiling]] 杠杆 B ③），benchmark 驱动、各自小 spec。
- **interp 侧同等优化**：interp 的 tagged-Value 寄存器文件是另一套，不在本 JIT change 内。
- **分配/GC 路径杠杆**（杠杆 A，malloc~31%）：与隔壁 [[unify-gc-heap-program]] 语义耦合，串行排后，非本 change。

## Open Questions（待 User 裁决）

1. ~~**本 DRAFT 覆盖 4 个 sub-phase，但 2C 风险显著高于 2.0/2A/2B。scope 到哪？**~~
   **✅ 已裁决（User 2026-08-15）：本 change 一路覆盖到 2C**（2.0→2A→2B→2C 全含）。头条 `s+=…` 循环的
   天花板突破在同一 change 内完成。**分 PR 落地纪律不变**：每 sub-phase 一个 PR、各自 benchmark 驱动、
   独立可回退；2C 因风险最高，其 PR 前仍做 `Z42_OSR_THRESHOLD=1` 全压测 + 每类循环形态覆盖（见 Phase 2C）。
2. **Phase 2.0 重构的验收标准**：要求 (a) 机器码逐字节不变（最严，可能因 CLIF 结构微调难达成），还是
   (b) e2e interp+jit byte-identical + self-host 5/5 + 无性能回归（行为等价即可）？倾向 (b)。
3. **收益门槛**：2A/2B 各自 land 前，用哪个 bench 锁收益？建议新增两个 scenario——「窄整数算术热循环」
   （锁 2A）+「块内算术链热循环」（锁 2B）——都用 long/int 免转换、避 helper-bound 混淆（方法论见
   [[measure-before-optimizing-and-nohup-trap]]）。OK 吗？
4. **change 命名**：`jit-unbox-regalloc` 妥否？（Cranelift 做实际 regalloc，我们做的是 unbox + SSA 提升
   + spill 布置——或叫 `jit-scalar-unbox`？）
