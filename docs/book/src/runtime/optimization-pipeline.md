# 优化管线（编译期 IR 优化 + 运行时 JIT/interp 分层）

> 对齐：2026-08-02（编译期 pass：const-fold / copy-prop / temp-DCE / **函数内联** + OptSet 门控已落地，
> change `add-compiler-inlining`）
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

**位置**：`z42c.semantics/src/IrOpt{Info,Pipeline}.z42` + `OptSet.z42` + `IrInline.z42`（compiler 源码，
非 stdlib z42.ir —— 只用 z42.ir 现有 public 字段 type-switch，零 bootstrap API-face 延迟）。挂
`IrGen.Generate` 末尾。

**OptSet：可独立开关的具名优化集（add-compiler-inlining）**：`Opt` 位集 `ConstFold=1/CopyProp=2/
Dce=4/Inline=8/All=15`。`IrOptPipeline.Run(m, optSet)` 逐 pass `if Opt.Has(optSet, X)` 门控——用户自助
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

**pass 2 copy-prop**：两相：
- **① producer-retarget**：SSA-lite lowering 系统性 emit `t = expr; copy local, t`（每个命名局部赋值一条 Copy）。
  相邻 producer→copy 且 t 单赋值(defs==1)单读(reads==1，那唯一读即本 Copy)、t≠local 时,把 producer 的 Dst
  retarget 成 local、删 Copy → `local = expr`。interp 每个赋值少一次 dispatch（热路收益大头）。
- **② use-site 级联传播（improve-copy-prop-cascade）**：producer-retarget 只吃**相邻**模式，`dst = copy src`
  中 src 无 producer 可 retarget（如 src 是形参 / 非相邻）时留存。级联相：对**单赋值** `dst = copy src`
  （dst 非形参、`defs[dst]==1`；src **稳定**=单赋值 temp `defs==1` / 从不重写形参 `defs==0`）建 `dst→src` 映射
  （**链式解析**到最终稳定 src），用 `IrOptInfo.ReplaceReads`（通用「按 remap 改写一条指令/终结子读操作数」，
  完整镜像 `AddReads` 读枚举）把**全函数** dst 使用点改写为 src、再删这些已死的 copy。
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
  （无地址/存储指令）。`_isInlinable`（判定）与 `_cloneRemap`（克隆）覆盖集**必须一致**。
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

> **未来方向（语言侧使能优化，展望非 spec）**：`readonly` / 不可变注解能在**分析够不到**处解锁更多优化——
> ① **`readonly` 字段 / 不可变对象** → 跨「不透明调用」的字段 load 提升 / CSE（分析无法证明任意调用后字段不变，
> `readonly` 契约可）；② **`in`/`readonly` 参数** → **跨包内联**：callee body 不可见时由**签名**携带「不写」保证；
> ③ **不可变局部（`let`/`val`）** → 更强 copy/const 传播 + 无别名推理。本 change 的只读实参直代入不需要它
> （过程内分析已够）；`readonly` 的价值点在**字段级 / 跨包**优化成为瓶颈时。属 lang 变更、phase-gated，届时独立 spec。

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
