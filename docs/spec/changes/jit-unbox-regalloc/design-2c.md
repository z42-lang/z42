# Mini-DRAFT: Phase 2C —— loop-carried 整数标量跨块寄存器驻留

> 状态：**DRAFT**（proposal 明确要求 2C 单独 mini-DRAFT + 强化验证后再 IMPL）。
> 前置：Phase 2A（整数原生快路径）+ Phase 2B（块内整数标量缓存 `RegCache`）。
> 性质：vm 类（纯 runtime JIT codegen，无格式 bump、自举字节不动）。

## 目标

打破 `s += …` 循环的 JIT≈interp 天花板：让**循环携带的整数标量**（累加器 `s`、归纳计数 `i`）
**跨迭代常驻 Cranelift SSA 值 / 机器寄存器**，循环体内零 `frame.regs` 往返。2B 只在单块内缓存（跨块
经内存），单次触及的 loop-carried 标量收益为零——正是本相要解的。

## 现状约束（核实 @ jit-unbox-2bc，IR dump `loop1.z42 sum`）

典型计数/累加循环 IR：
```
entry:  %2=const 0(s); %4=const 0(i); %8=const 1; br cond_0
cond_0: %5 = lt %4,%0; br.cond %5, body_1, end_2      [preds: entry, body_1(back-edge)]
body_1: %6=convert %4; %2=add %2,%6; %4=add %4,%8; br cond_0
end_2:  ret %2
```
- 循环头 `cond_0` 是带**回边**的块；loop-carried 整数 = %2(s)、%4(i)；%8 循环不变（entry 只 load 一次）。
- 每 z42 块 → 一个 `cl_blocks[]`；终结子是独立 `Terminator` 枚举；`seal_all_blocks()` 最后统一封（handles 回边）。
- 回边 `br cond_0`（target≤block_idx）+ `br.cond` 都 emit `emit_safepoint_check`。

## 关键洞察（使 2C 可行的三条）

### 洞察 1：safepoint **不需要** spill 整数（回边收益的前提）
回边每迭代有 safepoint。若 safepoint 前必 spill，就每迭代 spill → 零收益。但——
- GC 是**非移动**的（[[unify-object-byte-layout-program]] 路 A：标记指针 + 非移动），不重写 `frame.regs`；
- GC root 扫描只认堆 tag（Object/Array/Str），**整数槽（TAG_I64）被跳过**——即使 `s`/`i` 槽内存陈旧，
  GC 读到 TAG_I64 就略过，不读 payload、不做 root；
- 无 JIT→interp deopt 读整数寄存器（OSR 是 interp→JIT 单向；见洞察 3）。

∴ **整数 loop-carried 值可跨 safepoint 常驻、无需 spill**。这是 2C 相对 2B 的收益来源。

### 洞察 2：用 Cranelift `Variable` 让 Cranelift 自建 SSA/phi（避免手工 block-param 穿线）
Cranelift frontend 的 `Variable`（`declare_var`/`def_var`/`use_var`）在 `seal_all_blocks()` 时**自动**在
回边/汇合点插 block param（phi）。把 loop-carried 整数 reg 建成 Variable，就不必手工给
`cl_blocks[header]` 加 param、不必在每条入边 `jump` 塞 args——Cranelift 全包了。**这是把「最难的 SSA
构造」外包给 battle-tested Cranelift 的关键**，比手工 block-param 穿线安全得多。

### 洞察 3：OSR 变体**也驻留**——Cranelift Variable 自动补 block-param arg（实现时更正 v1 计划）
DRAFT 原计划「v1 禁用 OSR 变体驻留」，因担心 `translate_function` 的 OSR 分支
`jump cl_blocks[k], &[]`（**空 args**）与带 param 的循环头冲突。**实现时核实并推翻**：用 Cranelift
`Variable`（洞察 2）后，OSR 入口块跑同一 prologue——含 Variable 的**种子 load**（`def_var(var,
load frame.regs[reg])`，OSR 时 `frame.regs` 正是 interp 拷入的 live 状态 `from_interp_regs`）——再
`jump cl_blocks[k]`。Cranelift 的 SSA 构造在 `seal_all_blocks` 时**自动**把种子值作为循环头 block-param
的 arg **追加**到那条空-args jump 上（标准 use_var→append_block_param→rewrite-predecessor 算法），
于是循环头 phi 正确合并 `(OSR-entry 种子, 回边值)`。**故 OSR 变体照常驻留**，覆盖「单次调用内 OSR 进入
的热循环」——正是 headline `s+=…`（实测热循环几乎都走 OSR 变体：`SB.S.run osr=true`）。
**必须实测锁死**：`Z42_OSR_THRESHOLD=1`（强制每个循环走 OSR）下全测逐字节 == interp。

## 设计（实现版：整数 loop-carried，OSR 也驻留）

### 谁成为 Variable（**实现更简：无需循环检测**）
DRAFT 原设「loop-carried reg」需先做自然循环检测。**实现时简化并推翻**：用 Cranelift `Variable`
后，Cranelift 的 SSA 构造自动为跨回边的值插循环头 phi——所以**不必检测循环**。判据只剩：
1. `reg_types[reg].is_integer()`（I8..U64，物理 `Value::I64`）；且
2. **每一处**对该 reg 的访问都在「routed 白名单位置」（见 `compute_promotable_regs`）。

即**升格所有「访问全 routed」的整数 reg**——loop-carried 的自动获跨迭代驻留（2C），非 loop 的获块内
复用（≈2B），全交 Cranelift。非升格的 reg（堆值、F64/Bool/Char、任何被 memory-backed op 碰过的整数）
**保持 2A/2B 内存+缓存模型不变**。

### def_var / use_var 路由 + 内存同步纪律（正确性核心）
carried Variable 的**每一处**读写都必须走 Variable，且在内存边界双向同步：
- **写 carried var**（原生 int op / const / copy / field-get(int) / helper 结果）→ `def_var`。
  - helper/field/copy 写的是**内存** → 写后必 `def_var(var, load frame.regs[reg])` **reload**，否则 Variable
    与内存脱钩（⚠️ 已识别的「memory-sync 陷阱」：`x=foo(); while… x…` 若不 reload，use_var 看不到 foo
    的内存写、读到陈旧 prologue seed）。
- **读 carried var**（原生 int op）→ `use_var`。
  - helper 按 index 读它 → 调用前 **spill**：`store use_var(var) → frame.regs[reg]`。
  - 终结子 `br.cond`/`ret` 读它 → `use_var` 直接喂 brif / spill 到内存喂 hr_set_ret。
- **循环出口边** → spill（`store use_var → frame.regs`）供 end 块/后续/return 从内存读。
- **safepoint 前** → **不 spill**（洞察 1）。

### seeding（避免 use-before-def）
每个 carried Variable 在 prologue `def_var(var, load frame.regs[reg])` 种一次（param 从入参槽、局部从
Null 槽——被后续真正的 const/赋值 def 支配覆盖，dead seed 无害）。**但** memory-sync 陷阱要求：循环**前**
任何写该 reg 的内存型 op 都要 reload-def_var（见上），否则 seed 陈旧。

## 实施范围（实现版：per-reg 白名单，比「干净循环」更宽）
DRAFT 原拟「先只做干净数值循环」。**实现更宽且同样安全**：白名单是**逐 reg** 的，不是逐循环——
一个含 helper 的循环里，helper 碰的 temp 被 disqualify（留内存），但**累加器/计数器若只被原生
emitter 触及仍照常驻留**。故 `s += foo(i)` 这类含调用的循环，`s`/`i` 仍驻留（实测 `withcall` 通过、
`fieldacc` 的 `s += this.v` 驻留 s/i 得 1.35×）。内存同步陷阱被白名单从根上消除：promoted reg
**永不**被任何 memory-backed op 触及 → 无 mid-function spill/reload，唯二内存同步点 = prologue 种子
+ `Ret` spill。含 helper 循环**不需要** v2 的 around-call spill（helper 不碰 promoted reg）。

## 验证计划（proposal 强制）
- `Z42_OSR_THRESHOLD=1` 全压测：强制每个循环走 OSR 变体，确认 OSR 路径（禁驻留 fallback）逐字节 == interp。
- 每类循环形态覆盖：counted / while / nested / break-continue / 空体 / carried=param / carried=局部 /
  carried 被 helper 写（v2）/ 循环内 throw。
- `vm-jit-consistency`：490 e2e interp==jit 逐字节（主网）。
- 专项 bench：`sum`/`dot` 纯数值循环 JIT-2C vs JIT-2B A/B（锁收益，`measure-before-optimizing` 纪律）。
- cargo --lib + 自举 5/5 gen1==gen2 + stdlib。

## 风险与回退
- **最高风险**：白名单 `compute_promotable_regs` 必须与 codegen 的 routed 集合**精确一致**——白名单说
  promotable 但 codegen 某臂没走 use_var/def_var（或反之）= 静默 miscompile。故白名单枚举全 64 个
  Instruction 变体（未识别的新变体 → disqualify 其全部 reg，保守）+ 逐字节 vm-jit-consistency 兜底。
- **回退**：任何 reg 只要有一处 memory-backed 访问 → 不 promote → 走 2A/2B 内存模型（逐字节等价现状）。
  整个 2C 是 per-reg **纯增益开关**，某函数一个 reg 都不满足即完全回退 2B。

## 实测结果（实现完成）
- **正确性**：`sum`/`nested`/`break-continue`/`param-carried+early-return`/`helper-in-loop`/`unsigned-narrow`
  各形态 interp==jit 逐字节，**normal + `Z42_OSR_THRESHOLD=1`（强制 OSR）双模式**均通过；narrowint/chain
  回归通过。GREEN：cargo --lib 943/0 + e2e 490/0（含 OSR-forced e2e）+ 自举 5/5 gen1==gen2 + stdlib。
- **收益（JIT 2C vs JIT 2B A/B）**：纯算术累加环 `s = s + i*3 - seed`（20M 迭代）**1.75×**（335→192ms）；
  realistic `s += this.v; this.v += 1`（field 累加）**1.35×**（field 读写留内存，s/i 驻留）。**这是 2C
  相对 2B 的净收益——打破 loop-carried 单次触及标量的 `s+=…` 天花板**，2B 对这类零收益。
- **实现文件**：`jit/translate.rs`（`compute_promotable_regs` 白名单 + `load_int`/`store_int` 路由 +
  prologue 种子 + ConstI32/64 与 Ret 的 promoted 分支）；`Variable` 来自 cranelift-frontend。纯 runtime
  codegen，无格式 bump、自举字节不动。
