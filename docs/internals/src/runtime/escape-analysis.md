# 逃逸分析与栈上分配

> 对齐：2026-09-16（fix-ref-param-escape：`ref`/`out` 出口写回补进逃逸汇点 + golden `opt_all` sidecar）；2026-09-13（fix-stackalloc-misses-inlined-refs：栈 arena 根扫描补上字节内联引用那一半）；2026-09-03（unify-ir-operand-access：规则表兜底改为经统一操作数接口标全部读操作数，代码与本页「铁律」对齐）；2026-08-06（change `add-escape-analysis-stack-alloc` + `add-crossproc-escape-summary` 跨过程参数逃逸摘要）
> 对齐：2026-09-17（闭包栈分配先例并入本页：三档策略 + 运行时表示 + 编译期分析已消失的现状）
> 状态：🟡 编译期分析 + IR 标志 + interp 运行时（对象+数组）已实现；JIT 消费与跨过程精度为 future。
> **闭包**一支只剩运行时表示，编译期不再置标志（见末节）。

z42 的分配（`new Foo(...)` / `new T[n]` / `[a,b,c]`）默认走 GC 堆——region 分配锁 + 标记/清扫追踪
（profile 实测 interp 热路径 ~7% 在对象/数组分配）。其中相当一部分是**不逃逸的临时对象/数组**：只在
创建它的函数帧内被读写、从不流出。**逃逸分析**在编译期证明这一点，把这类分配改到**帧局部 arena** 上分配、
随帧退出即释放、完全绕过 GC。

这条范式 z42 先为**闭包**试过一次（`Value::StackClosure` + `Frame::env_arena`）——运行时那一半至今完整
保留，但**编译期那一半已经不在了**，所以闭包今天全部堆分配（见末节「闭包栈分配：运行时还在，编译期已停」）。
本机制把同一范式推广到对象与数组，并且这次把编译期分析做成了一个**可扩展规则**的 pass，以 `OptSet` 位独立开关。

## 总览

```
z42 源码 ──z42c──> z42 IR ──[IrEscapeAnalysis]──> IR(部分 alloc 带 StackAlloc=true) ──zbc──┐
                                                                                          │
   ① 编译期分析（z42c.semantics，引擎无关）                                                │
      · 流不敏感 may-escape 过近似（CFG-free）                                             │
      · 角色感知「逃逸汇点规则表」（可扩展点）                                              │
      · ctor this-escape 单函数摘要（对象合格前提）                                        ▼
   ③ interp 运行时消费（interp-first；JIT 忽略 flag 照常堆分配）      ┌──────────────────────┐
      ObjNew/ArrayNew(StackAlloc) ──► VmContext 栈 arena ──► Value::StackObject/StackArray  │
      FieldGet/Set·ArrayGet/Set/Len 识别栈句柄                    GC 在 safepoint 扫 arena 作根│
      帧退出 pop_frame LIFO 截断 arena → 释放                     诊断：frame_id 校验悬垂句柄 │
                                                                └──────────────────────────┘
```

## 机制 / 实现

### 编译期：`IrEscapeAnalysis`（`z42c.semantics`）

**流不敏感 may-escape 过近似（CFG-free）**：逃逸是「该 reg 的**任一**使用是否到达逃逸汇点」——与控制流
顺序无关的 may 问题，线性扫全函数即安全过近似，无需 CFG / 支配域（区别于 LICM，也天然规避「异常边不在
CFG」的坑）。

**引擎 `ComputeEscapedRegs(m, f, table, markRefWriteback)`**（三趟；`table`=跨过程摘要，解析 call 实参用）：
1. **Pass A**：逐指令 / 终结子，按操作数**角色**把「经逃逸角色读到的 reg」入种子集。
2. **Pass A′**：`ref`/`out` **形参写回汇点**（见下节；仅 `markRefWriteback=true` 时，即本函数内的栈分配判定）。
3. **Pass B**：copy 传递闭包不动点——`dst = copy src`，dst 逃逸 ⇒ src 逃逸（对象经 copy 流出）。

**角色感知逃逸汇点规则表（可扩展核心）**——逐操作数按角色分类。顺序即优先级：① 有摘要的静态调用 →
② **显式 neutral 白名单** → ③ 部分汇点（只标特定角色）→ ④ **兜底：经统一操作数接口（`IrInstr.ReadAt`）把
全部读操作数标逃逸**。neutral 必须显式登记；新增指令忘了登记 = 少一次栈分配机会，而非悬垂栈引用：

| 指令 / 终结子 | 逃逸角色（入种子） | 中性角色（不标） |
|---|---|---|
| `RetTerm` / `ThrowTerm` | 返回值 / 异常 reg | — |
| `FieldSet` / `StaticSet` / `ArraySet` | **value** reg | target / array / index |
| `ArrayNewLit` | 所有 **elems** | — |
| `CallInstr`（静态） / `ObjNew` 的实参 | **按 callee 摘要逐判**（跨过程，见下）；无摘要→全标 | callee 摘要说不逃逸的实参 |
| `VCall` / `CallIndirect` / `Builtin` / `CallNative` | 所有 args + receiver + callee（动态/原生，无摘要→全标） | — |
| `MkClos` | 所有捕获 reg | — |
| `AsCast`（结果别名）/ `ToStr`（调 ToString）/ `LoadLocalAddr`（取址）/ `IsInstance`※ / `Convert`※ | 其 Obj/Src/Slot | — |
| `FieldGet` / `ArrayGet` / `ArrayLen` | — | target / array / index |
| 算术·比较·位·移位·一元·StrConcat / `ArrayNew.Size` / Copy(Pass B) | — | 全部读操作数 |
| **规则表未列的任何指令** | **其所有读操作数（保守兜底）** | — |
| **被函数体重新定义过的参数槽**（Pass A′，非指令） | **该参数寄存器本身** | 从未被重定义的参数槽 |

> ※ `IsInstance.Obj` / `Convert.Src` 严格说中性（is-check / 恒等 convert 不泄露引用），但**有意标为逃逸
> 以收窄运行时触达面**——如此栈对象只经历 FieldGet/FieldSet、栈数组只经历 ArrayGet/Set/Len +
> FieldGet(`.Length`/`.Count` 经 FieldGet 路由)（+ Eq 走 `Value::PartialEq`），运行时无需为 IsInstance /
> `convert_value`（只 match 堆 Array/Object）补栈分支。代价：`p is T` / 引用恒等 cast 的操作数不栈分配
> （少量覆盖损失）。放宽 = 运行时补对应分支后移除该标记。

**铁律（对齐 LICM 的保守姿态）**：规则表**不认识**的指令读了目标 reg → **默认判逃逸**（over-approximate
安全兜底）。加精度 = 往规则表加/改一条分支，引擎（两趟）不动 —— 这是「后面可补规则」的落点。

> 2026-09-03 校正：unify-ir-operand-access 之前代码的实际兜底是「未列出 = neutral」（与本页铁律相反，靠人工
> 镜像 `AddReads` 枚举保完整）；现改为经接口标全部读操作数，代码与铁律一致。

### `ref`/`out` 形参的出口写回是逃逸汇点（change `fix-ref-param-escape`，2026-09-16）

**坑在于：callee 的 IR 里根本看不出哪个形参是 `ref`。** 运行时的 `ref`/`out` 走「入口 copy-in / 出口
copy-out」（`impl-ref-out-in-runtime`）——`exec_function_body` 在入口把持 `Value::Ref` 的参数寄存器解引用成
底层值（于是 callee 的 80+ 个指令 handler 完全不必感知 `Ref`），`run_ref_writebacks` 在**每条退出路径**上
把该寄存器的**终值**存回 caller 的 lvalue。`Param.IsRef` 只影响 **caller** 侧发 `load_local_addr`；callee
的形参寄存器类型不变、没有任何一条指令把「写回」表达成汇点。

结果：`void Fill(ref string[] a) { a = new string[2]; }` 里那个新数组的**全部使用都在本帧内**，规则表逐条
看过去都是中性的 → 判不逃逸 → 栈分配在 `Fill` 帧 → 写回给 caller 的是一个**已退出帧的句柄**。caller 一读就
炸：

```
Error: stack-alloc array handle used after its creating frame exited (idx=0, frame_id=19)
```

**规则**：Pass A′ 把**凡被函数体重新定义过的参数槽**一律标逃逸。写回的值必是某条 def 的产物，挡住所有
def 就挡住了所有可能被写回的分配；从未被重定义的参数槽不标——它的终值就是入口值，本就来自 caller、活得
比本帧久，标了只会白丢精度。

**为什么不精确识别 `ref`**：IR 层没有这个信息，要么给 `IrFunction` 加一组 per-param 标志并一路串到 zbc，
要么按「被重定义」保守近似。选后者——代价只是「非 ref 形参被重新赋值时也会挡掉一次栈分配」（`n = n + 1`
这类形参推进，挡掉的是给该形参传新分配的调用方），换来的是不依赖任何新元数据、对 zbc 解码回来的 IR 也
同样安全。

**这条汇点只在本函数的栈分配判定里开，摘要不开**（`markRefWriteback` 参数）。摘要问的是「**传进来的那个
值**是否逃逸」——参数槽事后被重新赋值，与入参值的去向无关；在摘要里一并标，会把 `while (n != null) { n =
n.Next; }` 这类「循环里推进形参」的常见写法判成参数逃逸，白白让**所有调用方**的实参丢掉栈分配。

> **为什么这个 bug 能活到 release 用户手上**：`--emit-zbc`（golden 用例的编译路径）的默认优化集**减掉了**
> `StackAlloc`（会改 golden 字节），于是 `src/tests/optimization/escape_*.z42` 整套在门禁里**一次也没开过
> 逃逸分析**——本页此前写的「专项单测覆盖」从来没被写出来过。同一 change 加了 `opt_all` sidecar
>（`z42c --emit-zbc --opt-all` → `Opt.All`）并给 `src/tests/optimization/` 全体挂上，这类用例才真正开始
> 测它们声称要测的东西。见 `src/tests/README.md` sidecar 表。

### 跨过程参数逃逸摘要（`IrEscapeSummary`，change `add-crossproc-escape-summary`）

**动机**：单函数分析里「传进任何调用的实参」一律判逃逸 → 最常见的「造临时对象传给只读它的辅助函数」
（`sum += Dist(new Point(i,i))`）享受不到栈分配。跨过程摘要打破这条保守。

**摘要 = 逐函数逐参 bool**：`paramEscapes[f][i]` = 函数 f 的**参数槽 i** 是否逃逸（`ParamEscapeTable`：
funcName→`ParamFlags(bool[ParamCount])`）。参数槽↔寄存器：`IrFunction.ParamCount` **含 this**、参数槽 i =
寄存器 i（实例方法 reg0=this=槽0）——故「param i 逃逸」= `ComputeEscapedRegs` 结果的 `esc[i]`。

**模块单调不动点（`Compute(m)`）**：摘要互相依赖（f 调 g）+ 递归 → 乐观初始化全 `false`，反复对每个有体函数跑
`ComputeEscapedRegs`（用**当前**摘要解析 call 实参）、把逃逸的参数槽置 `true`（**只增不减**），无变化即收敛。
单调保证终止；混沌迭代序不影响最小不动点。**返回参数天然逃逸**（`RetTerm.Reg` 被标 + copy 闭包传导）。

**消费（`_markEscaping` 精化）**：`CallInstr` 实参 `args[i]→callee 槽 i`、`ObjNew` 实参 `args[i]→ctor 槽 i+1`
（obj_new 前置 this=槽0）——按 callee 摘要逐个判，摘要说不逃逸即**不标**。**soundness 底线**：callee 找不到
（跨包）/ 无体 stub / `VCall`·`CallIndirect`·builtin（动态/原生，无可靠摘要）/ 实参越出摘要长度（varargs）
→ 该实参**保守全标逃逸**。宁可多标绝不漏标（漏标 = 悬垂栈引用）。

**对象的 ctor this-escape 前提**（`_ctorThisEscapes`）：`new Foo(a,b)` 带 `this`(槽0)调 ctor，对象合格需
ctor 不泄漏 this = 摘要 `table[ctor][0]==false`（跨包/无体/静态 ctor→保守判泄漏）。**这就是原单函数
`_ctorLeaksThis` 的推广**——ctor 只是「槽0=this」的普通函数，this-泄漏是槽0逃逸的特例，已并入通用摘要。

**对象完整合格条件**（三者皆满足）：① 结果 reg 本函数内不逃逸（用摘要解析 call 实参）；② 单赋值 temp
（`defs==1`）；③ ctor 摘要槽0不逃逸。数组无 ctor，只需 ①②。

**无运行时改动**：跨过程只让**更多**对象标 `StackAlloc`；`StackObject` 早已跨帧（per-context arena，ctor 子帧
即先例）→ 传进 callee 帧经 FieldGet 照常解析；非逃逸参数在 callee 内只读/写字段（存出去=逃逸已排除）→ 触达面
不变。误判（漏标逃逸）由运行时 frame_id 悬垂校验当场报错兜底 + `--no-opt stack-alloc` 开/关对拍在测试期抓。

**接入**（`IrOptPipeline.Run`）：模块级 pass，跑在所有 per-函数变换 + inline **之后**（分析匹配最终执行的
IR）。只置 `ObjNew/ArrayNew/ArrayNewLit` 的 `StackAlloc` 标志、不改指令流 → 不影响其它 pass，单独开也正确
（`OptSet` D2 独立性）。`Opt.StackAlloc=64`，release 开 / debug(-O0) 关；CLI `--opt/--no-opt stack-alloc`、
toml `[optimize] stack-alloc`。**dump/golden 路径排除**（`Opt.All - Opt.Inline - Opt.StackAlloc`）——同内联，
跨函数变换会改 golden 字节且脆弱，由真实 release 自建 + 专项单测覆盖。

### IR / 格式

三个分配指令加 `bool StackAlloc`（照 `MkClosInstr.StackAlloc` 先例）+ zbc 编码尾 `u8`。**bump zbc 1.28→1.29
/ zpkg 0.33→0.34**（version-bumping.md checklist）。

### 运行时：per-context 栈 arena（interp）

**为什么 per-context 而非 per-frame**：`new Foo` 的 ctor 在**子帧**执行、`this` 作 `Value` 传入；per-frame
arena 索引在子帧里无意义。**per-thread（per-`VmContext`）arena** 任何帧都能经 `ctx` 直取 → ctor 子帧天然
可解 `this`，无需跨帧机制（闭包用 per-frame `env_arena` 因其无 ctor 子调用，对象不同）。

- **句柄**：`Value::StackObject { idx, frame_id }` / `Value::StackArray { idx, frame_id }`（各 8B 内联，
  不撑大 24B `Value`）。`idx` 索引 `VmContext::stack_arena`（`Mutex<StackArena>`：owner 无竞争、GC 扫描在
  safepoint）。
- **分配**：`obj_new`/`array_new`(stack) 构造 `ScriptObject`/`ArrayObj` push 进 arena、返回句柄。ctor 照常在
  栈对象上跑（`this` = 句柄，FieldGet/Set 经 arena 解）。
- **访问**：FieldGet/Set、ArrayGet/Set/Len 识别栈句柄 → `ctx.stack_arena` 校验访问。栈对象字段存堆引用
  **不发 GC 写屏障**（栈对象非堆槽；其堆字段由根扫描保活）。
  **字段访问接单态 inline cache（`opt-stack-field-ic`）**：栈对象 FieldGet/FieldSet **复用堆路径同款
  `FieldIC`**（缓存 `TypeId→slot`）——`type_desc.id` 已解析、`field_index` 按类型定 slot，故 `(TypeId→slot)`
  缓存对堆/栈**同一份有效**。命中即直接 `slots[slot]`，跳过每访问一次的 `field_index` 字符串哈希查找。
  > 修正早期"栈访问非热路径、直接 hashmap 即可"的判断：对象**传进 callee 反复读字段**时哈希查找主导，使
  > 栈分配在该模式下反被堆（有 IC）反超；接 IC 后栈字段访问≈堆。实测密集字段访问 8M：**interp +5%**（jit
  > 不受影响——JIT 忽略 flag、对象走堆、本就用堆 IC）。
- **生命期（LIFO 截断）**：帧入栈 `push_frame` 记录 arena 长度基线（`VmFrame::stack_obj_base/arr_base`）；
  帧退出 `pop_frame` 截断回基线，bulk-free 该帧的栈分配。嵌套（对象 ctor 里再 `new`）自然 LIFO 正确。
- **GC**：`Value` 的 `trace_children` 视栈句柄为叶；外部根扫描器在 safepoint 扫 `ctx.stack_arena` 每个栈
  对象的字段 / 栈数组的 elems 作根（它们可能持堆 GcRef，必须保活）。arena 锁从不跨 GC 触发持有 → 不死锁。
  > ⚠️ **「栈对象的字段」是两半，缺一即悬垂**（`fix-stackalloc-misses-inlined-refs`，2026-09-13）：
  > `unify-object-byte-layout` PR-3 chunk 2b 把**直接的 object/array 字段**从引用侧表 `refs` 挪进
  > `bytes` 里的 8B 内联指针。堆一侧的 `Value::visit_gc_children` 同时读两半（`refs()` +
  > `trace_inline_refs`），而 `StackArena::scan_roots` 长期只读 `refs`——于是**一个不逃逸对象的数组字段
  > 不被任何根覆盖**：minor 在其 owner 还活着时就把它扫了，槽位复用后旧句柄静默解析到新住户
  > （debug 构建报 `GcRef::entry_ref: generation/alive mismatch`，release 构建直接答错对象）。
  > 现场就是 `xtask test` 自己：13 个 stage 全绿之后，耗时汇总死在自己的 `long[]` 上
  > （`long[]` 读出一个 `Char`）。**新增任何「对象引用存放位置」的表示，必须同时更新堆遍历与每个
  > arena 根扫描**——两者是同一条不变量的两个端点。
- **JIT（新分配）**：读得进新 zbc 的 `StackAlloc` 标志但**忽略**——`ObjNew`/`ArrayNew` 照常堆分配
  （`translate.rs` "JIT ignores stack_alloc in v1"）。interp-first（准则 1）：优化只服务无 Cranelift
  兜底的 interp；`interp==jit` 靠「输出相同、表示不同」成立。
- **JIT（OSR 继承的栈句柄）—— 必须处理**：⚠️ 曾误以为"一个对象整个生命期在同一引擎内 → JIT 永不遇到栈
  句柄"。**错**：**OSR 是函数中途 interp→JIT 切换**（`add-osr-loop-tiering`，`from_interp_regs` 拷
  `frame.regs`）。若 interp 段在**循环外**已栈分配一个对象/数组（`Value::StackObject/StackArray` 存于
  `frame.regs`），回边 OSR 进 JIT 后 JIT 代码会**继承并访问**该句柄。故 JIT 的字段/元素 helper
  **必须**镜像 interp 处理栈句柄：`jit_field_get`/`jit_field_set`（对象，复用 FieldIC、栈槽无 write
  barrier）与 `jit_array_get`/`jit_array_set`/`jit_array_len`（数组）各带一条 `StackObject`/`StackArray`
  臂，经 `ctx.stack_arena` 解析。原生内联字段/元素快路径的 hoist（`jit_obj_field_slot` /
  `jit_array_data_opt`）对非堆 receiver 返回 sentinel（`off=-1` / `ptr=null`）→ 路由到冷 helper，故修
  helper 即全覆盖。**漏这条 = OSR 下 `FieldGet/FieldSet/ArraySet…: expected object/array, got Stack*`
  崩**（默认 OSR 阈值高、`--release` 才开逃逸分析 → 平时 latent；见
  `fix-jit-osr-stackarray` #204 数组侧 / `fix-jit-osr-stackobject` 对象侧）。

### 诊断（栈分配出错要能第一时间知道）

栈分配的危险失败模式 = **悬垂栈引用**（逃逸分析误判 → 栈句柄活过创建帧 → 帧退出后被读）。静默即内存破坏。
多层防线把它变成**明确报错**：

1. **frame_id 校验（核心）**：帧退出截断后，句柄 `idx` 越界 → 报错；或槽被后续帧复用、`frame_id` 不符 →
   报错 `stack-alloc <kind> handle used after its creating frame exited … escape analysis miscompiled`。
2. **逃逸汇点 debug 断言（核心）**：在分析「声称」栈句柄永不到达的汇点（FieldSet/ArraySet/StaticSet 的 val、
   ArrayNewLit 的 elems）加 `debug_assert!` → 用运行时证据反证静态分析漏判。
3. **越界永远显式**：arena 索引一律 bounds-check。
4. **`Z42_STACKALLOC=off`**：运行期一键旁路（全堆分配，免重编 triage）；`=stats` 打印命中计数。

## 闭包栈分配：运行时还在，编译期已停

闭包是这条范式的**先例**（change `impl-closure-l3-escape-stack`，2026-05-02）。它的现状与对象/数组
那一半很不一样，必须写清楚，否则读代码会得出相反的结论。

### 三档策略（设计意图）

用户**永远只写一种代码**（`x => x + 1`），编译器按闭包出现的位置选实现档：

| 维度 | 档 A 栈分配 | 档 B 单态化 + 内联 | 档 C 堆擦除 |
|---|---|---|---|
| env 位置 | 调用方栈帧 | 不存在（内联展开）| GC 堆 |
| 堆分配 | 0 | 0 | 1（每次 `MkClos`）|
| 调用方式 | 直接 call | 内联 | 间接调用 |
| 代码膨胀 | 无 | 有（每闭包类型一份）| 无 |
| 跨线程 | ❌ | ❌ | ✅ |
| 典型位置 | 传给 `[no_escape]` 形参 | 泛型形参 `<F: (T)->R>` 的单态 call site | 存字段 / 入集合 / 作返回值 / 跨线程 |

设计上的决策算法是：泛型形参 → 档 B；具体函数类型形参且不逃逸 → 档 A；字段赋值 / 集合插入 / 返回值
→ 档 C；`var` 绑定 → 分析后递归归类；判不出来 → 档 C（保守）。

**今天只有档 C 是活的。** 档 A 曾有过一个子集实现、档 B 曾有过一个 alias 子集实现，两者的编译器侧
都已经不在仓库里（见下）。这意味着上表除了最后一列，当前全部是**意图**而非现状。

### 编译器侧：三个发射点全部写死 `false`

`MkClosInstr` 至今带着 `StackAlloc` 字段（`z42.ir/src/IrInstrCall.z42:206`，zbc 有对应的尾字节），
但**全部三个发射点都传常量 `false`**：

- `FunctionEmitter.z42:490`（局部函数升级成的闭包）
- `ExprEmitter.z42:356`（lambda 字面量）
- `CallEmitter.z42:504`（合成 thunk）

原来置位的那个 pass —— `ClosureEscapeAnalyzer`（TypeChecker 后置 pass：找 `var x = lambda;` 候选 →
扫函数体确认所有对 `x` 的引用都在 `BoundCall.Receiver` 位 → 写进 `SemanticModel.StackAllocClosures`
→ Codegen 透传）——**连同 `SemanticModel.StackAllocClosures` 一起已从 `src/` 全数消失**（两个名字
全仓零命中）。所以 `Value::StackClosure` 在今天的编译产物里**永不出现**。

同样消失的还有档 B 的 alias 子集：`TypeEnv._funcAliases`（把 `var f = Helper;` 折成 `Call "ns.Helper"`
的别名表）零命中。`var f = Helper;` 今天走的是 `ExprTyper.z42:104` 的 `BoundFuncRef` →
`ExprEmitter.z42:194-199` 的 `LoadFn`，是一个**函数指针值**，调用点仍是 `CallIndirect`——不是别名折叠。

> 这两件事一起说明：本页开头说的「先为闭包落地过」只对**运行时**成立。要复活闭包栈分配，正确的做法
> 不是重写 `ClosureEscapeAnalyzer`，而是把闭包接进本页的 `IrEscapeAnalysis`——`MkClos` 已经在规则表里
> （「所有捕获 reg 入种子」），缺的只是「`MkClos` 的**结果** reg 不逃逸时置 `StackAlloc`」这一条消费规则。

### 运行时侧：完整保留，随时可用

- **句柄**：`Value::StackClosure { idx: u32, frame_id: u32 } = 11`（`metadata/types/value.rs:94`）。
  8B 内联，载荷 `StackClosureData { env_idx, fn_name }` 在 `VmContext::transient_arena`
  （`value_aux.rs:66-69`）——与 `PinnedView` / `Ref` 同一套 make-value-copy 句柄化处理。
- **env 存放**：`env_idx` 索引**创建帧**的 `Frame::env_arena: Vec<Vec<Value>>`（`interp/frame.rs:17`）。
  `MkClos(stack_alloc=true)` 把捕获值 push 进 arena 并返回句柄（`exec_call.rs:415-416`）。
- **调用**：`CallIndirect` 从当前帧的 `env_arena` **物化出一个新的 GcRef** 交给 callee
  （`exec_call.rs:356-361`）——arena 里是裸 `Vec`，而 callee 的生命周期必须独立于 caller 帧，
  否则 caller 弹栈后就是 use-after-free。callee 因此完全不区分栈闭包与堆闭包。
  （对比 `Value::Closure`：堆闭包直接把已有的 env `GcRef` 交给 callee，引用计数 +1，零拷贝。）
- **GC 根**：⚠️ **不存在** `VmContext::env_arena_stack`。`unify-frame-chain`（2026-05-10）把
  `exec_stack` / `env_arena_stack` / `call_stack` 三个平行栈合并成单一的 `Vec<VmFrame>`
  （`exception/mod.rs:43`），push/pop 同步、没人能「只忘一半」。根扫描经 `VmFrame.env_arena`
  裸指针遍历每个 env 的每个 `Value`（`vm_context/construct.rs:318-324` 与 `394-400` 两处，
  分别对应两种 visitor 签名）。

### 与「每次迭代新绑定」的关系

reference 那条「循环变量每次迭代是新绑定」的语义**由值快照规则自动满足**，codegen 不需要为循环做
任何特殊处理：`MkClos` 在创建时把当前迭代的值**拷进** env，而不是持有对循环变量存储的引用。即使
`for` / `foreach` 复用同一个寄存器存循环变量，每次创建的闭包 env 仍是互相独立的快照。
回归防护在 `src/tests/closures/closure_l3_loops.z42`。

### 未做 / 待定

- **`[no_escape]` 形参标注**：档 A 的关键依赖——stdlib 高阶 API 的形参必须显式标注，否则分析只能把
  传进去的闭包保守归到档 C。这是结构性属性，不是用户级断言。语法与属性都尚未引入。
- **`--warn-closure-alloc`**：一个「报告每个落到档 C 的闭包字面量及其原因」的编译选项，**从未实现**，
  CLI 里没有这个 flag。
- **无捕获 lambda 的降级**：无捕获闭包在 IR 层降成函数引用（`FuncRef`），但用户视角统一是 `(T) -> R`
  ——z42 **不引入**独立的函数指针类型。

## 判定与扩展（后续可补规则）

同一「规则表 + 引擎」框架的 future 扩展（引擎不动，改规则 / 加运行时分支）：
- **跨过程参数逃逸摘要**（模块不动点）：让方法调用后仍不逃逸的对象合格（放宽 ctor 单函数摘要 + IsInstance）。
- **字段敏感 / 部分逃逸**。
- **标量替换**（把对象炸成寄存器彻底消除分配）作第二种 lowering。
- **JIT 侧 arena 落地**。

延后条目登记见 `docs/roadmap.md` Deferred Backlog Index（`escape-stack-future-*`）。

## 关联文档
- 开关 / 管线位置：[optimization-pipeline](optimization-pipeline.md)
- 闭包的用户面捕获语义：[闭包与捕获语义](../../../reference/src/language/closures.md)
- 闭包栈分配先例：change `impl-closure-l3-escape-stack`（`docs/spec/archive/`）
- 格式 bump：[version-bumping.md](../../../agent/rules/version-bumping.md)
- 引入：change `add-escape-analysis-stack-alloc`（`docs/spec/`）
