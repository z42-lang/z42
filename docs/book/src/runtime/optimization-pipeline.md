# 优化管线（编译期 IR 优化 + 运行时 JIT/interp 分层）

> 对齐：2026-09-03（unify-ir-operand-access：读/写寄存器枚举走 `IrInstr` 统一操作数接口）；2026-08-07（编译期 pass：const-fold / copy-prop / temp-DCE / **函数内联** / CSE / LICM /
> **逃逸分析栈上分配** / 循环分配复用 / **readonly 字段读优化** / **纯函数调用优化** /
> **const 常量传播 + 死分支消除** + OptSet 门控已落地）
> 状态：🟡 编译期 IR 优化已成形（4 pass + 可独立开关 OptSet）；运行时 JIT 分层随 `jit-lowering-pipeline` 续。

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
   - 运行时分析(加载期/JIT 期)不能拖垮加载与执行;能编译期算好的静态分析就别留到运行时反复算(见 [self-hosting](../../../design/compiler/self-hosting.md) 的 REGT 先例:reg_types 编译期算好,运行时只消费)。
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

**位置**：`z42c.semantics/src/IrOpt{Info,Pipeline}.z42` + `OptSet.z42` + `IrInline.z42`（compiler 源码，
非 stdlib z42.ir —— 只用 z42.ir 现有 public 字段 type-switch，零 bootstrap API-face 延迟）。挂
`IrGen.Generate` 末尾。

> **逃逸分析栈上分配（`Opt.StackAlloc=64`，change `add-escape-analysis-stack-alloc`）**：不逃逸的
> `ObjNew`/`ArrayNew`/`ArrayNewLit` 改到帧局部 arena 分配、绕过 GC。分析算法（CFG-free 规则表引擎）、
> 运行时 per-context arena、诊断防线详见 [逃逸分析与栈上分配](escape-analysis-stack-alloc.md)。

> **基于 sealed 的去虚化（`Opt.Devirt=2048`，change `add-sealed-devirt`）**：receiver 静态类型是
> 本地非泛型 **sealed 类** → 目标编译期唯一 → `CallEmitter._emitCall` **emit 时就地**把 `VCallInstr`
> 降级为直接 `CallInstr`（天然在 `IrInline` 前，解锁 virtual 方法内联；`VCall` inline pass 吃不进）。
> 目标解析不确定即回落 VCall（永不 miscall）。机制与 v1 边界详见 [sealed 修饰符 · 去虚化](../language/sealed.md)。
> 注：这是**唯一的 emit 时优化**（其余在 `IrOptPipeline` post-emit）——因去虚化需 receiver 的**静态类型**，
> 而 lowering 后的 IR `VCall` 已不携带该信息。

**OptSet：可独立开关的具名优化集（add-compiler-inlining）**：`Opt` 位集 `ConstFold=1/CopyProp=2/
Dce=4/Inline=8/Cse=16/Licm=32/StackAlloc=64/LoopAllocReuse=128/ReadonlyLoad=256/PureCall=512/DeadBranch=1024/Devirt=2048/All=4095`。`IrOptPipeline.Run(m, optSet)` 逐 pass `if Opt.Has(optSet, X)` 门控（Devirt 例外：emit 时门控）——用户自助
勾选任意子集（**非**「高档含低档」的单调档位）。profile 默认：**debug=None**（`-O0`，忠实可调试）、
**release=All**（发布最高优化）；解析优先级 CLI（`--opt`/`--no-opt`）> toml `[optimize]` > profile。
**独立性硬约束（D2）**：每个 pass 单独开启都必须正确——允许「增效依赖」（inline 后 dce 删得更多），
**禁止「正确性依赖」**（任何 pass 不得假设别的 pass 先跑过才不出错）。落地检查 = 每 pass 各自重算
reads/defs + 单测逐 pass 单独开跑 golden。**顺序**只影响效果不影响正确性：inline 靠前（产更多下游
机会）、清理类（const-fold/copy-prop/dce）靠后。

> **dump / golden 路径特例**：`IrDump._buildF` / `BuildModuleD`（codegen 单测、`--dump-ir`、golden .zbc
> regen）用 `Opt.All - Opt.Inline`——单函数 dump 断言的是「单函数本地优化后」IR，内联是跨函数变换、会
> 折叠含直接调用的 golden 且脆弱，故排除；该值 = 引入内联前 `Opt.All` 的等价输出，既有 golden 逐字节
> 不变。内联行为由真实 release 自建（D7）+ `DumpFuncOpt(src,key,optSet)` 专项单测覆盖。

**读/写计数**：一趟扫全函数，`reads[reg]`（每指令读操作数 + 每块终结子读）、`defs[reg]`（每指令定义寄存器）。**枚举来源 = z42.ir 的统一操作数接口**
（unify-ir-operand-access：`IrInstr.DefReg / ReadCount / ReadAt`、`IrTerminator.ReadReg`）——每条指令自己回答操作数，
`IrOptInfo` 不再逐 opcode 镜像 `ZbcWriter._regtInstr`（REGT 收集同走该接口），新增指令实现接口即被所有 pass 正确计入。参数寄存器 seed 为 live-out（out/ref 参数最终值由调用方读，函数内看不到）。

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

**pass 2 copy-prop**：两相：
- **① producer-retarget**：SSA-lite lowering 系统性 emit `t = expr; copy local, t`（每个命名局部赋值一条 Copy）。
  相邻 producer→copy 且 t 单赋值(defs==1)单读(reads==1，那唯一读即本 Copy)、t≠local 时,把 producer 的 Dst
  retarget 成 local、删 Copy → `local = expr`。interp 每个赋值少一次 dispatch（热路收益大头）。
- **② use-site 级联传播（improve-copy-prop-cascade）**：producer-retarget 只吃**相邻**模式，`dst = copy src`
  中 src 无 producer 可 retarget（如 src 是形参 / 非相邻）时留存。级联相：对**单赋值** `dst = copy src`
  （dst 非形参、`defs[dst]==1`；src **稳定**=单赋值 temp `defs==1` / 从不重写形参 `defs==0`）建 `dst→src` 映射
  （**链式解析**到最终稳定 src），用 `IrOptInfo.ReplaceReads`（通用「按 remap 改写一条指令/终结子读操作数」，
  经接口 `SetReadAt` / `SetReadReg`）把**全函数** dst 使用点改写为 src、再删这些已死的 copy。
  安全：src 稳定 ⇒ 其值在 dst 每个使用点相同（IR 有效=def 支配 use；src 单赋值故值恒定）。`ReplaceReads` 同为 CSE 复用。

**pass 2b CSE（公共子表达式消除，`Opt.Cse`；跑在 const-fold 后、copy-prop 前）**：**块内** value-number——
对纯计算 op（arith/cmp/位/一元/convert）建 key `op|操作数ids`，同块同 key 复现即「重复计算」。用
`IrOptInfo.ReplaceReads`（cascade 引入的读改写基建）把重复结果的所有使用点改写为**首个**结果、删重复指令。
`(a+b)*(a+b)` → 只算一次 `add`。**块内**限定（同块 firstDst 在前 → 支配 dupDst 及其使用点，值恒等），
跨块不做（避支配分析）。
> **安全边界**：① 操作数须**稳定**（单赋值 temp `defs==1` / 从不重写形参 `defs==0`）→ 两次出现同值；
> dst 须单赋值（remap 有效）。② `Div`/`Rem` 可入——首个在前，若 trap 则控制流到不了第二个，否则同值。
> ③ 含**分配 / 副作用 / 可空解引用**者（`StrConcat`/`FieldGet`/`ArrayGet`/`Call*`/`*Set`/`ObjNew`/…）**不入**
> key（值不由操作数唯一决定，或有可观察副作用）。④ `convert` key 含目标类型标签（同 src 不同目标不可复用）。

**pass 2c LICM（循环不变量外提，`Opt.Licm`；跑在 const-fold 后、cse 前）**：把自然循环体内**纯 + 循环不变**的
计算提到循环 pre-header（每进循环只算一次），interp 收益大头（循环主导运行时）。机制（保守 v1，`IrLicm.z42`）：
⓪ **有异常表（try/catch/finally，`ExcCount>0`）的函数整体跳过**——CFG 仅从 `Br/BrCond/Ret` 终结子建，
**不含异常隐式边**（受保护区→handler/finally），支配/循环分析会错 → 误提 → miscompile（event/multicast/
finally/div-by-zero 等 e2e 曾中招）。异常边入 CFG 后再放开；
① CFG（各块终结子取后继 Br→1/BrCond→2/Ret·Throw→0，反转得前驱位阵）；② **支配**（迭代数据流
`dom[b] = {b} ∪ (∩ dom[preds])`，entry=Blocks[0]）；③ **回边**（边 b→h 且 h 支配 b）→ 自然循环，body =
h + 从 latch 反向可达（不过 h）（**并**同 header 所有回边体，多 latch 才完整）；④ **pre-header 保守判定**：
h 的**唯一**循环外前驱 `ph` 且 `ph` 终结子 = `br h` → 干净 pre-header，否则**跳过该循环**（不做 CFG 手术造
pre-header）；⑤ **不变量**：循环体内 IsPure + **单赋值 dst** 指令，其**所有读操作数不在「循环支配域」定义**
（"循环支配域" = **被 header 支配的所有块**——含 `throw`/多出口退出块，见下）；⑥ 外提到 pre-header 尾
（终结子前）、从循环体删（v1 单层——被提指令互不依赖 → 任意序外提安全；链式不变量留下游/再跑一遍）。
> **⚠️ 支配域判不变（关键正确性）**：判「操作数是否循环内定义」**必须用 header 支配域**（`dom[b*n+h]`）而**非**
> 「latch 反向可达的自然循环体」——后者**排除**了循环内的**异常/多出口退出块**（`throw` 块无后继、到不了 latch），
> 而这些块仍受 header 支配。若用反向可达体，退出块里定义的寄存器会被漏判为「循环外」→ 误提到 pre-header →
> 运行期 `undefined register`（曾在 `TopoOrder` 嵌套循环+throw 触发）。所有真循环块都受 header 支配 ⊆ 支配域，
> 故用支配域**保守安全**（至多少提循环外后继块的定义，绝不误提）。外提**源**仍用自然循环体（只移真循环内指令）。
> **安全边界**：仅提 **IsPure** 指令（白名单排除 Div/Rem 除零陷阱、FieldGet/ArrayGet NPE·越界、Call*/*Set 副作用）
> → 提到「可能零迭代」的 pre-header 也安全（纯值、不触发陷阱/副作用）。单赋值 temp（`defsGlobal==1`，多定义局部
> 提了会丢环内赋值）+ pre-header 支配循环 → 提前定义仍支配所有使用点，正确。处理序（回边按块序）确定 → 不动点收敛。

**pass 2e 循环内分配 hoist + 对象复用（`Opt.LoopAllocReuse=128`，change `add-loop-alloc-hoist-reuse`；模块级，跑在 escape 之后）**：
把循环体内**每迭代 `new` 的临时对象/数组**——若**迭代内可复用**——hoist 到 pre-header **只分配一次** + 循环体
**重初始化**，消除 escape-栈分配的**帧 arena 累积**（量测：循环体 stack-alloc 从 1.01× 提到接近 per-call）+ 把
堆分配的 N 次 malloc+GC 降到 1 次。复用 `IrLoopUtil`（LICM 抽出的 CFG/支配/自然循环/干净 pre-header 机件）。
机制（保守 v1，`IrLoopAllocReuse.z42`）：
- **资格 5 条**（任一不足即跳过，安全兜底）：**C1** `StackAlloc==true`（逃逸分析已证不逃逸函数；对象另含
  「ctor 不泄漏 this」，可复用 ⟹ 不逃逸，故 C1 必要且直接复用已有结果）；**C2 迭代内局部**——alloc 的 dst
  （单赋值 temp）的**前向 copy 闭包**不含任何**多赋值 reg（defs>1）**，否则对象引用被循环携带（`head=new Node(i,head)`
  / `prev=p`），复用会让携带别名看到改写 → miscompile；**C3 形状固定**——`ArrayNew.Size` 不在循环体内定义
  （循环不变；LICM 已把不变 const 提到 pre-header）；**C4 重初始化完整**——对象 = ctor **单基本块**（字段写无条件、
  每迭代覆写；未被写字段保持裸分配零初始化 = 与 fresh 一致），数组 = 常量下标**读前写全**（单块线性扫：ArraySet
  常量下标入已写集、ArrayGet 常量下标要求已写、动态下标/其它读 → 失败）；**C5** 干净 pre-header 的自然循环。
- **变换（无格式 bump、无新指令）**：**对象** `%r = ObjNew(Cls, ctor, [args])` → pre-header 追加 `%r = ObjNew(Cls, ctor="", [])`
  （**空 ctor 名哨兵 = 裸分配**，走运行时 `obj_new` 的 `outcome=None` 路径：`func_index.get("")` 皆 None → 跳过
  ctor、只分配），循环体原址 → `Call ctor(%r, [args])`（静态调 ctor，`this=%r` 前置——运行时 obj_new 内部本就
  `exec_function(ctor,[obj,...args])`；dummy dst 防覆写 `%r`，bump `MaxReg`）；**数组** 整条 `ArrayNew` 移到 pre-header
  （本就是裸分配，循环体既有元素写回重初始化）。保留 `StackAlloc` 标 → arena 只分配一次、帧退出释放。
- **正确性（SSA 支配）**：dst 单赋值、定义支配所有使用点 → 变换后 ctor-Call/元素写仍在原址（支配所有读）→ 每迭代
  「先重初始化再读」，跨迭代无脏值。**主门 = `--no-opt loop-alloc-reuse` 开/关逐字节对拍**（e2e golden）；
  纯编译期变换，无运行时开关/断言（初稿的运行时旁路已删——运行时拿到的已是变换后 IR）。IR dump 用
  `Opt.All - Opt.Inline - Opt.StackAlloc - Opt.LoopAllocReuse` 隔离（golden 稳定）。
> **v1 未覆盖（design Deferred）**：`ArrayNewLit`（字面量元素需写手术）；嵌套循环只 hoist 到内层 pre-header
> （处理序决定，仍正确、收益略逊全外提）；数组动态下标 / 变长 Size / 多块使用。与 scope/回边 arena 复位互补。

**pass 2f readonly 字段读优化（`Opt.ReadonlyLoad=256`，change `add-readonly-fields-opt`；piggyback CSE + LICM，`ReadonlyLoad` 单独门控）**：
`readonly` 字段构造后不变（类型检查强制：仅声明类实例 ctor 内经 `this` 或字段初始化器可赋值，否则 `E0415`），给
优化器一个可信契约——`FieldGet` 从「不可 CSE/LICM」（`IsPure` 因 NPE/可变排除）解禁为可消重/可外提。
- **信息通路（无 zbc 格式 bump）**：`FieldGetInstr.Readonly` 是**纯内存标志**，emit 时由 `ExprEmitter` 从
  `FieldSymbol.IsReadonly` 填；优化器在 `ZbcWriter` **之前**消费，序列化的 `field_get` 字节不变 → 不 bump zbc/zpkg、
  不触发两代自举。代价：跨 zpkg 导入字段拿不到 readonly（保守当非 readonly，Deferred）。
- **块内 CSE**（`IrOptInfo.CseKey` 的 `fget|<objId>|<field>` 分支）：同一稳定接收者上对同一 readonly 字段的重复
  `field_get` ⇒ 同值 ⇒ 复用首个。**失效**：函数内该字段有任何 `field_set`（即 ctor 内多次写）时跳过其 readonly
  CSE（`_collectWrittenFields`，保守正确——普通方法无 readonly 写 → 全消重）。
- **LICM 外提**（`IrLicm._isHoistableReadonlyFget`）：仅提**接收者 = `this`（reg0，实例方法恒非空）**的 readonly
  `field_get` 到 pre-header——`this` 非空杜绝「零迭代时 NPE 时机漂移」；且循环体内该字段无 `field_set`（值不变）。
  params/locals 接收者留 Deferred（需非空/支配分析）。
- **正确性主门**：`src/tests/optimization/readonly_field_hoist/`（开/关输出必须一致）。**实测 interp ~1.87×**
  （热循环每迭代 3 次 `this.x` 读被外提+消重到 ~0；bench `readonly_field_bench.z42`）。

**pass 2g 纯函数调用优化（`Opt.PureCall=512`，change `add-pure-call-opt`；piggyback CSE + LICM，`PureCall` 单独门控）**：
`IsPure` 白名单把**任何 `Call` 判为不纯** → 用户函数调用永不进 CSE/LICM。**自动推断**同模块函数纯度
（`IrPureFunctionTable`），解禁纯调用的消重/外提。**无用户标注**（`pure` 关键字 Deferred）。
- **纯度推断（模块不动点）**：`IrPureFunctionTable.Compute(m)` 对标 `IrEscapeSummary`，方向相反——**乐观全纯 →
  发现副作用降级 → 单调收缩收敛**（递归纯函数因乐观初值自然判纯；StrMap 无 Remove → 每轮重建）。
  `pure(f)` ⟺ f 每条指令是「`IsPure` 白名单内」或「对纯函数的 `CallInstr`」或「**readonly 字段读**
  （`FieldGetInstr.Readonly`，**复用 readonly change 的标志**——`int scale(C c,int k){return k*c.f;}` 读
  readonly `c.f` 即纯）」，**且无块以 `throw` 终结**。其余（写字段/静态/数组、读非 readonly 字段·静态·数组、
  **分配**、`Div/Rem`、IO、`VCall`/动态派发、调非纯）→ 非纯。imported/无体 → 保守非纯。
- **纯度定义为何这么窄**：CSE/LICM 假设「同参同结果」——读可变外部状态会破坏它，故排除非 readonly 字段/数组读；
  **分配排除**因 CSE 消重会改对象身份（`==`/GC）；**no-throw** 因 LICM 提到可能零迭代 pre-header，会抛的
  「纯」函数提前执行 = 异常时机漂移。
- **CSE**（`IrOptInfo.CseKey` 的 `call|Func|argIds` 分支）：同 callee + 全 args 稳定的纯调用消重（纯 = 不依赖
  可变状态 → **无需失效表**，比 readonly 简单）。**LICM**（`IrLicm._isHoistablePureCall`）：全 args 循环不变的
  纯调用提到 pre-header。
- **与 inline 的分工**：小函数被 `Inline` 抢先消化（展开成算术，常规 CSE/LICM 处理）；pure-call 的价值在
  **非内联函数**（大 / **递归**）。**正确性主门** `src/tests/optimization/pure_call_hoist/`（开/关一致）。
  **实测 interp ~200×**（递归 `fib(23)` 循环不变调用被外提：OFF 4.24s → ON 0.02s；bench `pure_call_bench.z42`）。

**pass 2h 常量条件死分支消除（`Opt.DeadBranch=1024`，change `add-const-keyword`；跑在 const-fold 后、licm/cse 前）**：
喂料来自 [`const` 编译期常量](../language/const.md)替换（`const bool` 引用 → `ConstBoolInstr`）与 const-fold
（常量比较 → `ConstBoolInstr`）。分两步：
- **① 折叠**：块终结子 `br.cond(cond, T, F)` 且 `cond` 由**单赋值** `ConstBoolInstr` 产出 → 折成无条件
  `br(命中分支)`。**始终安全**（条件跳转→无条件跳转，不移块）；折掉的 `cond` 读消失，其 `ConstBool`
  producer 由后续 temp-DCE 清。
- **② 移块**：**仅 `ExcCount==0`** 的函数，从 entry(block0) 沿 `Br/BrCond` 后继做可达性 BFS，移除不可达块
  （如恒假 `if` 的 then 块）。
- **⚠️ CFG 铁律**：z42 IR 的 CFG 只从终结子建、**不含异常隐式边**（try→catch/finally）——故 `ExcCount>0`
  的函数**只折不移**（贸然移块会删掉 handler 可达的块 → miscompile；镜像 LICM 的 `ExcCount>0` 跳过）。移块
  安全性：有效 IR 里 def 支配 use → 可达块引用的寄存器其定义块必可达，移不可达块不产生悬垂读。
- **正确性主门** `src/tests/optimization/const_dead_branch/`（开/关行为一致——死分支本就不该执行）。

**pass 3 temp-DCE**：删「IsPure 白名单内(不抛/不调用户码/不写内存/不分配) + Dst 全函数零读 + 非参数」的死指令。
Div/Rem(除零陷阱)、FieldGet/ArrayGet(NPE/越界)、Call*/*Set 等不在白名单 → 保留。

**pass 0 函数内联（`IrInline`，`Opt.Inline`；跑在清理 pass 之前）**：模块级逐 caller 展开合格直接调用点。
IrModule = 单 CU，callee 在同模块内解析（函数共享 StringPool → 内联后 `const.str` 池索引仍有效）。

- **资格（D4，`_eligibleCallee`）**：① 直接 `CallInstr`（`VCall` 不入本 pass）；② 同模块按名解析到 callee；
  ③ 非递归（`callee.Name ≠ caller.Name`）；④ callee **单块**且终结子 = `RetTerm`（v1 不做多块重贴标签/续延块）；
  ⑤ 无异常表 / 无 varargs / 实参数 == 形参数（精确 arity，避开默认值·params 打包）；⑥ curated 体（见下）；
  ⑦ `instrCount ≤ 24` **或**全模块单调用点（恒内联）；⑧ per-caller 预算 `INLINE_CALLER_BUDGET=256`。
- **v1 curated 指令集**：z42 IR **无统一「重映射一条指令寄存器」操作**（每种指令各有 `TypedReg` 字段）→
  内联逐指令类型克隆 + 重命名。v1 只支持直线 curated 集：const*（i32/i64/f64/bool/char/str/null）/ copy /
  算术（add/sub/mul/div/rem）/ 比较（eq/ne/lt/le/gt/ge）/ 位·一元（bit_and/or/xor/shl/shr/not/neg/bit_not/
  convert）/ field_get。callee 体含任一**非** curated 指令（call/vcall/obj_new/array_*/field_set/static_*/
  str_concat/...）→ **跳过整个 callee**。curated 集天然排除嵌套调用（无递归展开）、分配、副作用存储、ref/out
  （无地址/存储指令）。`_isInlinable` 只表达「哪些指令内联安全」的**策略**；`_cloneRemap` 经接口 `Clone` + `SetDefReg` / `SetReadAt`
  重映射，对全部指令通用（unify-ir-operand-access 之前二者是必须手工同步的两条平行链）。
- **展开（D5）**：`offset = caller.MaxReg`（fresh reg 区，> 所有 caller reg → 无碰撞）；callee reg `r` → caller
  reg `r+offset`。① **形参绑定（clean-inline-copies）**：**只读**形参（body 从不写它）→ body 中直接**代入调用方
  实参寄存器**、不 emit copy（只读形参只作读操作数、从不是 dst → 代入安全，等价调用点求值）；**被写**形参
  → `copy (p+offset), arg[p]` 材料化到可写 fresh reg（body remap 到 `p+offset`）。免去 v1 每形参一条 `copy`
  给 interp 增回的 per-arg dispatch，且让常量实参的内联算术直接被 const-fold 折叠（`Add(2,3)`→`const 5`）；
  ② body：单块每条 curated 指令克隆 + 按重映射（只读形参→实参 / 其余→`+offset`）；
  ③ `Ret` 有值 → `copy call.Dst, remap(retReg)`（绑返回值；只读形参直返时 remap 即实参），void → 无；④ `CallInstr`
  被上述序列原地取代；⑤ **reg_types 同步扩**（`callee.RegTypes[r] → caller.RegTypes[r+offset]`，否则内联后 typed
  指令 / JIT i64 特化失效）；⑥ **稳定序**（按 block/instr idx 顺扫 + 声明序 remap 确定）→ 输出确定（自举不动点前提）。
- **传导内联（emergent）**：按声明序逐 caller 就地改写，先定义的 callee 若在其自身处理时被内联成 curated
  直线体，随后的 caller 会看到并可再内联它 → 沿声明序向后自然传导。对声明序确定、每函数处理一次 + 预算
  → 终止有界。
- **自举字节不动点（D7）**：内联纯优化、**不新增语法 / 不改 zbc·zpkg 格式** → 不触发 bootstrap-seed 两阶段。
  稳态 gen1==gen2（确定性稳定序）；**引入当次**种子（无内联）编当前源 → gen1 未内联、gen2（gen1 编）已内联
  → gen1≠gen2 破一代，gen2==gen3 自愈。self-host byte-identical 是 opt-in soak（非默认 GREEN gate）+ pair-gen
  兜底 → 不阻塞发布链。
- **多块 callee（inline-multiblock）**：放宽「单块」限制——含控制流（if/loop → `br`/`br.cond` + 多 `Ret`）的
  多块 curated callee 也可内联。资格：**每块**终结子 ∈ {Ret,Br,BrCond}（Throw 不可）+ 全 curated 指令 + 总
  指令数门。展开分两 Phase：**A** 单块 callee 就地 splice（原路径）；**B** 多块 callee **split+insert**——拆 caller
  块为 `head`（前半 + 被写形参 copy + `br entry`）与 `cont`（后半 + 原终结子），中间插 callee 各块（唯一
  relabel `__il<ctr>_`、指令 clone+remap、`Br`/`BrCond` 目标 relabel、每 `Ret` → 绑返回值 + `br cont`）。
  被写形参 copy 放 head（只执行一次 → loop 安全）；只读形参直代入实参。唯一标签前缀按处理序递增 → 确定
  → 自举不动点收敛（引入当次因多块内联面骤增，gen2 明显大于 gen1、破一代，重建 gen2==gen3 自愈）。
- **v1 仍保守（后续 spec 放宽）**：跨包内联、单态 `VCall` 内联、放宽阻断特征（异常表 / ref-out）。
- **只读实参直代入（clean-inline-copies）**：callee 只读形参（body 从不写它）→ body 中直接代入调用方**实参
  寄存器**、不 emit `copy`；被写形参才 `copy` 材料化。免去每形参一条 copy 给 interp 增回的 per-arg dispatch，
  且让常量实参的内联算术直接被 const-fold 折叠（`Add(2,3)`→`const 5`）。只读性由**过程内分析**（`_writtenParams`
  扫 callee body）判定——callee body 全可见时无需注解。

> **语言侧使能优化**：`readonly` / 不可变注解在**分析够不到**处解锁更多优化——
> ① **`readonly` 字段** → 字段 load 提升 / CSE：**同模块已落地**（pass 2f `Opt.ReadonlyLoad`，change
> `add-readonly-fields-opt`）；**跨 zpkg 导入字段**仍 Deferred（需 zbc/zpkg 格式 bump 把 readonly 位写进
> `IrFieldDesc`）；② **`in`/`readonly` 参数** → **跨包内联**：callee body 不可见时由**签名**携带「不写」保证（展望）；
> ③ **不可变局部（`let`/`val`）** → 更强 copy/const 传播 + 无别名推理（展望）。②③ 属 lang 变更、phase-gated，届时独立 spec。

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
- 自举与 REGT 先例(编译期算好、运行时消费的模式):[self-hosting](../../../design/compiler/self-hosting.md)
- JIT 惰性逐函数编译:[jit-lazy-compile](jit-lazy-compile.md)
- 引入/演进:change `jit-lowering-pipeline`（`docs/spec/changes/`）
