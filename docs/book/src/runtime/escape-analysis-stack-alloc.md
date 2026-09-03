# 逃逸分析与栈上分配

> 对齐：2026-09-03（unify-ir-operand-access：规则表兜底改为经统一操作数接口标全部读操作数，代码与本页「铁律」对齐）；2026-08-06（change `add-escape-analysis-stack-alloc` + `add-crossproc-escape-summary` 跨过程参数逃逸摘要）
> 状态：🟡 编译期分析 + IR 标志 + interp 运行时（对象+数组）已实现；JIT 消费与跨过程精度为 future。

z42 的分配（`new Foo(...)` / `new T[n]` / `[a,b,c]`）默认走 GC 堆——region 分配锁 + 标记/清扫追踪
（profile 实测 interp 热路径 ~7% 在对象/数组分配）。其中相当一部分是**不逃逸的临时对象/数组**：只在
创建它的函数帧内被读写、从不流出。**逃逸分析**在编译期证明这一点，把这类分配改到**帧局部 arena** 上分配、
随帧退出即释放、完全绕过 GC。

这条范式 z42 先为**闭包**落地过（`Value::StackClosure` + `Frame::env_arena`）；本机制把它推广到对象与数组，
由一个**可扩展规则**的编译期 pass 驱动，以 `OptSet` 位独立开关。

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

**引擎 `ComputeEscapedRegs(m, f, table)`**（两趟；`table`=跨过程摘要，解析 call 实参用）：
1. **Pass A**：逐指令 / 终结子，按操作数**角色**把「经逃逸角色读到的 reg」入种子集。
2. **Pass B**：copy 传递闭包不动点——`dst = copy src`，dst 逃逸 ⇒ src 逃逸（对象经 copy 流出）。

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

> ※ `IsInstance.Obj` / `Convert.Src` 严格说中性（is-check / 恒等 convert 不泄露引用），但**有意标为逃逸
> 以收窄运行时触达面**——如此栈对象只经历 FieldGet/FieldSet、栈数组只经历 ArrayGet/Set/Len +
> FieldGet(`.Length`/`.Count` 经 FieldGet 路由)（+ Eq 走 `Value::PartialEq`），运行时无需为 IsInstance /
> `convert_value`（只 match 堆 Array/Object）补栈分支。代价：`p is T` / 引用恒等 cast 的操作数不栈分配
> （少量覆盖损失）。放宽 = 运行时补对应分支后移除该标记。

**铁律（对齐 LICM 的保守姿态）**：规则表**不认识**的指令读了目标 reg → **默认判逃逸**（over-approximate
安全兜底）。加精度 = 往规则表加/改一条分支，引擎（两趟）不动 —— 这是「后面可补规则」的落点。

> 2026-09-03 校正：unify-ir-operand-access 之前代码的实际兜底是「未列出 = neutral」（与本页铁律相反，靠人工
> 镜像 `AddReads` 枚举保完整）；现改为经接口标全部读操作数，代码与铁律一致。

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
  对象的 slots / 栈数组的 elems 作根（它们可能持堆 GcRef，必须保活）。arena 锁从不跨 GC 触发持有 → 不死锁。
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

## 判定与扩展（后续可补规则）

同一「规则表 + 引擎」框架的 future 扩展（引擎不动，改规则 / 加运行时分支）：
- **跨过程参数逃逸摘要**（模块不动点）：让方法调用后仍不逃逸的对象合格（放宽 ctor 单函数摘要 + IsInstance）。
- **字段敏感 / 部分逃逸**。
- **标量替换**（把对象炸成寄存器彻底消除分配）作第二种 lowering。
- **JIT 侧 arena 落地**。

延后条目登记见 `docs/roadmap.md` Deferred Backlog Index（`escape-stack-future-*`）。

## 关联文档
- 开关 / 管线位置：[optimization-pipeline](optimization-pipeline.md)
- 闭包栈分配先例：change `impl-closure-l3-escape-stack`（`docs/spec/archive/`）
- 格式 bump：[version-bumping.md](../../../../.claude/rules/version-bumping.md)
- 引入：change `add-escape-analysis-stack-alloc`（`docs/spec/`）
