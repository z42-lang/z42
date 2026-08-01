# 优化管线（编译期 IR 优化 + 运行时 JIT/interp 分层）

> 对齐：2026-07-30（设计立项，随 change `jit-lowering-pipeline` 落地）
> 状态：🟡 架构设计中 —— 本页先固化**两条设计准则**与**两层架构**；具体 pass 随实现补「机制/实现」节。

z42 有两条优化路径,共用同一份 z42 IR,但优化的位置和消费方式不同:

```
z42 源码 ──z42c──> z42 IR
                     │
   ┌─────────────────┴── 【编译期 IR 优化】(引擎无关,做一次,烘进/伴随 zbc)
   │     const-fold / DCE / copy-prop / …  → 更精简的 IR
   │
   ├──> interp : dispatch 优化后 IR（指令更少 = dispatch 更少）
   └──> JIT    : lower 优化后 IR → 原生 + Cranelift 二次优化（GVN/LICM/DCE 在原生层）
```

- **编译期 IR 优化**:引擎无关,两引擎共享。z42c 现在**零 IR 优化**(朴素 codegen),这是主要缺口。
- **运行时优化**:JIT 的 Cranelift 原生层优化 + 分层执行(interp → JIT 热函数升级)。

---

## 设计准则（必须始终遵守）

> 这两条是本管线所有设计/实现决策的**总准则**,任何 pass、任何分层策略都要过这两关。

### 准则 1：编译期 emit 的 IR，优化目标是 interp（interp-first）

**IR 层优化以「让 interp 执行更快」为第一准绳,而非以 JIT 友好为准绳。**

理由:
- **interp 没有 Cranelift 兜底,IR 层是它唯一的优化来源**。IR 里每留一条冗余指令,interp 每次执行/每次循环迭代都要多 dispatch 一次。
- **JIT 有 Cranelift 二次优化**:const-fold / DCE / LICM / GVN 在原生层 Cranelift 已经做了。IR 层为 JIT 做这些是重复劳动,IR 层优化对 JIT 至多省一点编译时间。

由此的具体推论:
1. **首要目标 = 减少 interp 要 dispatch 的指令条数**(每条指令 = 一次解释开销)。copy-prop 消冗余 `Copy`、DCE 删死指令、const-fold 折叠常量,都直接砍 dispatch 次数。
2. **指令形态选择偏向 interp 执行快的形态**(合并、专用指令减少每条的解释成本),即使该形态对 JIT 无所谓。
3. **不为 JIT 牺牲 interp**。若某优化让 IR 更适合 Cranelift 但增加了 interp 的 dispatch,不做 —— 那类优化归 JIT 自己的 lowering 层。

### 准则 2：运行时优化必须控制内存与时间开销；分层升级后旧层内存要能清理

**任何运行时优化(尤其 JIT 分层)都要为它多占的内存和多花的时间负责。**

具体约束:
1. **分层升级后,旧层的指令/代码内存要可回收**。一个函数从 interp tier 升级到 JIT tier 后,若其 interp IR / 旧 JIT 代码不再需要,**其内存必须能被释放,不能无限累积多份表示**。分层是「同一函数多种表示并存」,内存管理是分层的第一等公民,不是事后补丁。
2. **优化本身的时间/内存开销要有度**:
   - 编译期 pass 不能拖垮编译(pass 复杂度要与收益匹配;先测收益再投)。
   - 运行时分析(加载期/JIT 期)不能拖垮加载与执行;能编译期算好的静态分析就别留到运行时反复算(见 [self-hosting](self-hosting.md) 的 REGT 先例:reg_types 编译期算好,运行时只消费)。
3. **分层策略要权衡「升级成本 vs 收益」**:只有热函数值得升级(升级有编译+内存成本);冷函数留在 interp。分层决策要有明确的升级触发条件与降级/清理路径。
4. **回收要池化,避免频繁向 OS 申请/归还内存**。分层升级/回收不得退化成 malloc/mmap 抖动 —— 回收的内存进 **free-list / arena 复用**(下次加载/升级重用),而非回收即还 OS、再要再申请。仓内先例:`REGS_POOL`/`FRAME_POOL`(线程本地寄存器文件 free-list)。JIT code 页、IR blocks 回收同理:按块池化复用,批量申请,减少系统调用次数。

> **为什么把这两条钉死**:z42 是 interp/JIT/AOT 混合执行的 VM。很容易犯的错是「优化只盯着 JIT(因为 Cranelift 显眼)」而让 interp 吃朴素 IR,或者「加了分层却让每个函数常驻多份表示把内存吃爆」。这两条准则分别堵这两个系统性陷阱。

---

## 两层的职责边界

| 优化 | 层 | interp 收益 | JIT 收益 | 备注 |
|------|----|:---:|:---:|------|
| 常量折叠 / DCE / copy-prop / CSE | **编译期 IR 层** | **大(唯一来源)** | 小(Cranelift 已做) | 遵准则 1:为 interp 做 |
| 循环 / 支配 / 不变量分析 | 编译期 IR 层(或伴随 zbc 的元数据) | 快路缓存 | hoist 决策 | 单一真相,两引擎共用 |
| GVN / LICM / 原生 const-fold | **JIT 原生层(Cranelift)** | — | 自动 | 不在 IR 层重做 |
| 分层升级 / 代码缓存 / 内存回收 | **运行时** | — | — | 遵准则 2:内存可清理 |

---

## 机制 / 实现

**位置**：`z42c.semantics/src/IrOpt{Info,Pipeline}.z42`（compiler 源码，非 stdlib z42.ir —— 只用 z42.ir 现有
public 字段 type-switch，零 bootstrap API-face 延迟）。挂 `IrGen.Generate` 末尾，逐函数跑 pass。

**读/写计数**：一趟扫全函数，`reads[reg]`（每指令读操作数 + 每块终结子读，镜像 ZbcWriter._regtInstr 保完整）、
`defs[reg]`（每指令 Dst）。参数寄存器 seed 为 live-out（out/ref 参数最终值由调用方读，函数内看不到）。

**pass 1 const-fold**：`TryConstFold(ins, cint, cval)` —— 建单赋值 int 常量表(`ConstI64`→`_parseIntLit`)，
前向扫描把两操作数皆常量的运算/比较就地折成 `Const` 指令，并把折出的新 const 登记回表(链式传播
`1+2→3`、`3+3→6`)。放最前 → 产出的 const 供 copy-prop 传播、死源 const 供 temp-DCE 清理。
**这是"加规则 = 插一条 rule"的样例**：`TryConstFold` 是一张 opcode→折叠规则表,加规则只是往这张表加
分支,pass 框架不动。**已扩展：单操作数代数恒等式**(2026-08-01)——双常量折不成时再试:`x+0`/`0+x`/`x-0`/
`x*1`/`1*x`/`x/1`/`x|0`/`x^0`/`x<<0`/`x>>0` → `copy` 另一操作数;`x*0`/`0*x`/`x&0`/`x%1` → `const 0`
(经 `_cvIs(cint,cval,r,v)` 判"是否已知常量 == v")。`→copy` 不新增死值;`→const 0` 让操作数可能变死,
但 temp-DCE 只删 `IsPure` 指令,有副作用的 producer 仍保留。**后续可继续加**:常量字符串长度、常量数组
长度等。收益本身偏低(真实代码字面量运算少),价值在**证明管线可持续扩展**。
> **安全边界(规则扩展时必守)**：整数算术**仅非负结果才折**(`_parseIntLit` 编码不了负数,负值保守跳过)；
> div/rem 防除零、shift 防越界(镜像 `IrGenFacts._foldBinary` 的 `long` 语义——它在 long 里算、对全宽度
> emit,是被生产验证过的做法)。float 暂不折(文本化影响自举字节一致)。每条新规则配「折叠生效 + 安全不折」
> 双向用例(见 `codegen_tests.z42` const-fold 段)。

**pass 2 copy-prop**：SSA-lite lowering 系统性 emit `t = expr; copy local, t`（每个命名局部赋值一条 Copy）。
相邻 producer→copy 且 t 单赋值(defs==1)单读(reads==1，那唯一读即本 Copy)、t≠local 时,把 producer 的 Dst
retarget 成 local、删 Copy → `local = expr`。interp 每个赋值少一次 dispatch（热路收益大头）。

**pass 3 temp-DCE**：删「IsPure 白名单内(不抛/不调用户码/不写内存/不分配) + Dst 全函数零读 + 非参数」的死指令。
Div/Rem(除零陷阱)、FieldGet/ArrayGet(NPE/越界)、Call*/*Set 等不在白名单 → 保留。

**正确性边界**：只碰单赋值 temp（命名局部重赋值需 def-use，留后）；单趟不级联（保守）。一个寄存器值 escape
函数的途径 = 返回 / out·ref 参数 / 有副作用指令读，三者齐全 DCE 才安全（out_var 回归即漏 out 参数 live-out）。

> **⚠️ 跨子系统坑（runtime）**：编译期删指令会改变「哪些寄存器被指令引用」。JIT 的 `max_reg`（给 frame.regs
> 定尺寸）曾只从指令流反推 dst，漏了异常表 catch_reg → copy-prop 删掉最后引用该 reg 的指令后 JIT 帧越界 panic
> （interp 因 frame.set 自动扩容免疫）。已修（`translate.rs` max_reg 补扫 catch_reg + 折入 func.max_reg）。
> **教训：任何从指令流反推寄存器集的运行时分析，遇到 IR 优化都可能暴露不完整 —— 以编译器权威 reg 数/显式表为准。**

> 待补:const-fold；分层升级触发条件与旧表示回收路径（准则 2 运行时面，change `runtime-jit-tiering`）。

### z42c 寄存器模型对优化的影响（关键前提）

- **表达式临时寄存器单赋值**(每个 temp 全新 `Alloc`,不复用)→ const-fold / temp-DCE / copy-prop **几乎不需分析即可做**。
- **命名局部变量重赋值**(`x = x+2` 经 `CopyInstr` 拷回同一寄存器,"SSA-lite")→ 局部变量的 DCE/const-prop 需 def-use 分析,较重,留后。
- 因此**低垂果实排序**:先啃单赋值 temp 上的优化(易、interp 直接受益),局部变量的重分析留后阶段。

## 关联文档
- 自举与 REGT 先例(编译期算好、运行时消费的模式):[self-hosting](self-hosting.md)
- JIT 惰性逐函数编译:[jit-lazy-compile](jit-lazy-compile.md)
- 引入/演进:change `jit-lowering-pipeline`（`docs/spec/changes/`）
