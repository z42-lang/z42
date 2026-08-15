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

### Phase 2B — 块内标量 unbox + 机器寄存器缓存（block-local）✅ 已实现

**做（实际实现，比 DRAFT 更收敛）**：`reg_access.rs` 加 `RegCache`——**只缓存整数标量（I8..U64，
物理全 `Value::I64`、tag `TAG_I64`）的 i64 payload**（Bool/Char/F64/堆值不缓存、直落内存，避免 tag
多态与 GC 可见性复杂度）。`load_i64` 命中缓存返回驻留 SSA 值、否则从内存 load 并记 clean 条目；
`store_i64` 只更新缓存 + 标脏（不写内存，延到下次 flush）。五个整数 emitter
（`emit_i64_binop`/`_convert`/`_neg`/`_bit_not` 走缓存读写；`_cmp` 走缓存读、结果 Bool 直写内存 +
`invalidate(dst)`）改用之。

**flush（spill 脏 + 清空）汇点**——由 `translate.rs` 统一在**非参与缓存的指令前**触发（`instr_uses_int_cache`
返 `false` 即 flush），覆盖 DRAFT 枚举的全部汇点：

- **块终结子前**（terminator 是独立 `Terminator` 枚举、在指令循环后单点 flush——跨块值走内存，本 phase 不引入 block param）；
- **每个 Category-B helper / Call / VCall / ObjNew / Builtin / const / copy / field / array / bool 前**
  （它们直接读写 `frame.regs` 或按 index 调 helper）——`check!` 宏拆 Cranelift 块的点也必是 helper 调用、
  已被 flush 覆盖，故**缓存 SSA 值永不跨 Cranelift 块边界**；
- **每个 z42 块开头**：新建空 `RegCache`（前驱已在其 terminator flush → 内存权威）。

`emit_safepoint_check` 出现在 Br/BrCond/Call 后，均已被上面的 terminator/helper flush 覆盖。

**正确性不变式**：在任何「缓存参与整数 op 之外的东西可能读/写 `frame.regs`」的点，内存都 coherent。
`instr_uses_int_cache` 必与各 op 快路径谓词逐一对齐（otherwise 非参与 op 未 flush = 读到脏内存 bug）。
**OSR 无关**（OSR 只在块入口进入，缓存从块头 load 起步）。空缓存 flush（`std::mem::take` 空 BTreeMap）
**发零条机器指令** → 控制流密集代码无回归。

**收益范围（实测诚实标注）**：
- **理想场景**——长直线算术块、值重度复用（`sline.z42`：a/b/c/d/e/f 层层复用、无分支无调用）：**JIT 2B
  比 JIT 2A 快 1.30×**（5.35s→4.12s，σ 极小），byte-identical。这是「块内多次触及同一值省掉中间内存
  往返」的直接兑现。
- **控制流/调用密集代码**（`chain.z42`：含 if 分支 + 中途 helper 调用）：**基本持平**（2A/2B 差 ~3% 在
  噪声内）——分支与调用把基本块切碎到复用距离之下，块内缓存跨不过去。**单次触及的 loop-carried 标量
  （`s+=…`）收益为零**——那是 2C（跨块 block-param 驻留）的领域。

即：2B 是**数值/表达式内核**的真实局部胜利，对一般控制流代码中性无害。

### Phase 2C — loop-carried 跨块寄存器驻留（真正的天花板突破）✅ 已实现

> mini-DRAFT + 落地机理见 [`design-2c.md`](design-2c.md)（含 OSR 决策更正、白名单、内存同步陷阱）。

**做（实现版，比 DRAFT 更简更宽）**：用 **Cranelift `Variable`** 承载 loop-carried 整数标量——
Cranelift 的 `use_var`/`def_var` + `seal_all_blocks` **自动**在循环头插 phi、在前驱边（含 OSR 空-args
jump）追加 block-param arg。**故不必手工 threading block-param、不必检测循环**（DRAFT 原计划的两大难点
被 Cranelift 外包掉）。

- **谁驻留**：`compute_promotable_regs` 白名单——整数 reg 且**每一处访问**都在 routed 位置（const-int /
  原生 int 算术·比较·convert / `Ret`）。任何 memory-backed op（copy/field/array/call/helper/struct/…）
  碰过的 reg 一律 disqualify → 留 2A/2B 内存模型。**per-reg 粒度**：含 helper 的循环里累加器/计数器仍驻留。
- **内存同步只两点**：prologue 种子（`def_var(var, load frame.regs[reg])`，OSR 时种 interp 拷入的 live
  状态）+ `Ret` 前 spill。**safepoint 不 spill**（非移动 GC 跳整数槽）——这是相对 2B 的每迭代收益来源。
- **OSR 照常驻留**（DRAFT 原拟禁用，实现推翻）：Cranelift 自动补 OSR 空-args jump 的 block-param arg，
  循环头 phi 合并 `(OSR 种子, 回边值)`。热循环几乎都走 OSR 变体，故这一步是 headline 收益的关键。

**收益（JIT 2C vs JIT 2B A/B）**：纯算术累加环 `s=s+i*3-seed`（20M）**1.75×**（335→192ms）；realistic
`s += this.v; this.v += 1`（field 累加）**1.35×**。**打破 2B 对 loop-carried 单次触及标量的 `s+=…`
零收益天花板**。正确性：全循环形态（nested/break-continue/param-carried/helper-in-loop/unsigned）
**normal + `Z42_OSR_THRESHOLD=1` 双模式**逐字节 == interp；e2e 490/0（含 OSR-forced）+ 自举 5/5 + stdlib。

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
