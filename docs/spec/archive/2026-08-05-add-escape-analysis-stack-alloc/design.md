# Design: 逃逸分析驱动的栈上分配

## Architecture

三层，接入点极小，全部对齐两个既有先例（IR 层 `MkClosInstr.StackAlloc`；运行时 `Value::StackClosure`
+ `Frame::env_arena`）。

```
z42 源码 ──z42c──> z42 IR ──[IrEscapeAnalysis]──> IR(部分 alloc 带 StackAlloc=true) ──zbc──┐
                                                                                          │
   ① 编译期分析（z42c.semantics，引擎无关，做一次）                                        │
      ├─ 角色感知逃逸汇点规则表（可扩展点）                                                 │
      ├─ 流不敏感 may-escape 过近似（CFG-free）                                            │
      └─ ctor this-escape 单函数摘要                                                       │
                                                                                          ▼
   ③ 运行时消费（interp-only，準则 1）                        ┌──────────────────────────────┐
      ObjNew/ArrayNew(StackAlloc) ──► Frame arena ──► Value::StackObject/StackArray         │
      FieldGet/Set·ArrayGet/Set/Len 识别栈变体                GC: 扫 arena slots 作根，不 sweep │
      JIT: 读 flag 但忽略 → 照常堆分配（语义等价）            帧退出 → arena drop → 释放         │
                                                            └──────────────────────────────┘
```

**接入点（编译器 4 处，与 LICM 完全同构）**：
1. `OptSet.z42`：`Opt.StackAlloc=64`、`All`=127、`ByName("stack-alloc")`。
2. `IrOptPipeline.Run`：模块级 pass，`if (Opt.Has(optSet, Opt.StackAlloc)) IrEscapeAnalysis.Run(m)`。
3. `IrEscapeAnalysis.z42`：pass 本体。
4. IR 指令字段 + zbc 编码（z42.ir）。

## Decisions

### Decision 1: 运行时落地 = 帧 arena 栈对象（非标量替换）

**问题**：不逃逸怎么变现？
**选项**：
- A **帧 arena 栈对象**：新 `Value::StackObject/StackArray`，slots 存帧 arena，GC 跳过，帧退出释放。
  通用（对象+数组一套机制）、匹配 `StackClosure` 先例；代价=新 Value 变体波及 FieldGet/Set + GC + zbc bump。
- B **标量替换**：编译器把对象炸成寄存器，彻底无分配、无格式 bump、最省 dispatch；但变换最难
  （要内联 ctor、重写所有字段操作），适用面最窄。
**决定**：**选 A**（User 裁决）。理由：一套机制覆盖对象+数组、复用已验证的 StackClosure 运行时范式、
风险可控。B 作为 future 第二种 lowering 纳入同一规则框架（Out of Scope）。

### Decision 2: v1 消费仅 interp，JIT 读取忽略 flag（interp-first）

**问题**：JIT 要不要也走 arena？
**选项**：A JIT 也做 arena（改 translate + helper + JIT 字段访问快路）；B JIT 忽略 flag、照常堆分配。
**决定**：**选 B**。理由（准则 1，见 optimization-pipeline.md）：IR 层优化以 interp 为第一准绳，interp
无 Cranelift 兜底、是唯一优化来源；JIT 有原生层。JIT 堆分配一个"本该栈分配"的对象，**输出语义完全
相同**（字段值一致），只是表示不同 → gauntlet `interp==jit` 成立（比较输出非表示）。这把 JIT 侧改动
压到"读得进新 zbc 字段即可"，彻底规避 JIT 内联数组快路与栈变体的冲突。JIT arena 落地 = future。

> 一个对象的整个生命期在**同一引擎**内（一次函数调用要么 interp 要么 JIT，tiering 以函数为粒度切换、
> 不在对象存活期间跨引擎倒手）→ 不会出现"interp 建的栈对象被 JIT 代码当堆对象解引用"。

### Decision 3: 对象合格的前提 = ctor this-escape 单函数摘要（不递归）

**问题**：`new Foo(a,b)` 的 `ObjNew` 内部会带 `this` 调用 ctor。按"传给 call 即逃逸"的规则，ctor 调用
会让**所有对象判逃逸 → 一个都不合格**。数组无 ctor 无此问题。对象怎么合格？
**选项**：
- A **完整跨过程摘要 + 模块不动点**：为每个函数算参数逃逸摘要，ctor/方法调用按被调方摘要决定实参是否
  逃逸。最精确，但要模块级不动点 + 递归处理，v1 复杂度/风险高。
- B **ctor 单函数摘要，不递归**：只对 ctor 函数体做**单函数**的 param-0(this) 逃逸检查（复用同一规则表引擎）。
  ctor body 内"把 this 传给另一个函数/存字段/返回" → 判 this 逃逸 → 该类对象不合格。**不追进被调方**
  （保守：ctor 调别的函数传 this = 逃逸）。
**决定**：**选 B**。理由：CFG-free、无模块不动点、bounded（每 ctor 算一次可缓存）；合格集=「ctor 只对
this 做字段初始化、不外泄 this」的对象（struct 式临时的主流形态），保守但正确。A 的模块不动点摘要作为
**future 第一条扩展规则**（同一引擎升级、框架不动）。

> **对象的完整合格条件**（三者皆满足）：① 该 `ObjNew` 结果 reg 在本函数内不逃逸（规则表判定，含
> "任何方法调用的 receiver = 逃逸"）；② 结果 reg 是单赋值 temp；③ ctor 的 this 摘要 = 不逃逸。
> ⇒ v1 合格对象只经历 FieldGet/FieldSet + 中性读，从不被调方法、从不流出。运行时栈变体的触达面因此
> **被静态收窄到 FieldGet/FieldSet**（+ 数组 get/set/len），大幅压缩运行时改动与风险。

### Decision 4: 分析 CFG-free、流不敏感（over-approximate）

**问题**：要不要像 LICM 那样建 CFG + 支配域？
**决定**：**不需要**。逃逸是「**该 reg 的任一使用**是否到达逃逸汇点」——流不敏感的 may 问题，无关控制流
顺序即安全过近似。算法=线性扫全函数指令/终结子，按操作数角色分类每个读；worklist 传播 copy 传递性。
比 LICM 简单一个量级，且天然规避 LICM 那套「异常边不在 CFG」的坑（本 pass 根本不看 CFG）。

### Decision 5: 加 flag 而非新 opcode

**问题**：`ObjNewInstr` 加 flag，还是新增 `NewObjectStack` 指令？
**决定**：**加 flag**（照 `MkClosInstr.stack_alloc` 先例）。两种方式都是 wire 变更、格式代价相同；flag
省 opcode 空间、复用现有 ctor/分配分发逻辑。zbc 尾字节 `u8`（reader `read_u8()!=0`）。

### Decision 6: 对象栈分配的 ctor 跨帧墙 —— 运行时落地方案（🔴 待 User 裁决，2026-08-05 实施期发现）

**问题（实施期硬发现）**：D1 定的「帧 arena 栈对象」照搬 `StackClosure` 的**每帧 arena**
（`Frame::env_arena`）。但闭包与对象有本质差异：

- **闭包**：env 在**创建帧内**就地构建、`CallIndirect` 也在**同帧**消费 → 帧相对索引 `env_idx`
  永远在创建帧内解析，成立。
- **对象**：`new Foo(a,b)` 的 `ObjNew` 会在**新子帧**里跑 ctor（[exec_object.rs:96-99]：
  `exec_function(ctor, [this, ...])`），`this` 作为 `Value` 传入。若 `this = Value::StackObject(idx)`
  是**创建帧相对**的 arena 索引，则在 ctor 子帧里 `idx` 指向**子帧自己的 arena** → `this.x = a`
  写错地方。

`StackClosure` 从没踩这个坑（闭包无 ctor 子调用）。**数组无 ctor、无方法** → 元素在同帧由
`ArrayNewLit`/后继 `ArraySet` 填、只被同帧 `ArrayGet/Set/Len` 读 → **零跨帧、零问题**。

**选项**：

#### 方案 A —— 数组优先（运行时 v1 仅数组）

- **落地**：仅 `Value::StackArray(u32)` 变体，索引 `Frame::stack_arr_arena: Vec<ArrayObj>`
  （复用现成 `ArrayObj`）。`array_new`/`array_new_lit`(stack) → push 帧 arena、返回 `StackArray(idx)`；
  `ArrayGet/ArraySet/ArrayLen` 识别之；GC `trace_children` 视为叶、根扫描器扫 `stack_arr_arena` 元素；
  帧 drop → arena Vec drop。**全程同帧、与 StackClosure 先例同构。**
- **编译器**：把 `IrEscapeAnalysis` 收紧为 v1 只标 `ArrayNew`/`ArrayNewLit`（跳过 `ObjNew` 标记）→
  语义诚实（标志=真能栈分配）。对象作 follow-up 再放开（pass 引擎已支持，改一行门控即可）。
- **工作量**：types.rs +1 变体 + 各 exhaustive `match Value` 补臂（trace_children/PartialEq/Debug/
  Display/is_heap_ref[已 `_=>false`]/GC scan，~10–20 处机械补臂）；interp/mod.rs Frame +1 字段 +3 处 init；
  exec_array.rs 分叉；exec_instr.rs 透传；vm_context.rs 根扫描 +1 段；arc_heap.rs match 补臂。
- **风险**：**低**。数组自包含、无跨帧、无 ctor、无 ref-out（passing→逃逸已排除）；主要 ripple = Value
  变体的 exhaustive match 补臂（机械、编译器会逐处报）。
- **覆盖/价值**：**中**。数组常被返回/存入（逃逸），非逃逸数组多为局部 scratch；且 packed 数组已优化
  过（#107/#109/#111）。捕获面可能**小于对象**（对象临时如 Point/tuple/小 record 更常见）。

#### 方案 B —— 对象+数组，跨帧句柄（index-based，推荐用 B2）

- **落地**：`Value::StackObject{frame_idx: u32, idx: u32}`（内联 8B，无需 Box）。跨帧解析：新增
  `ctx.stack_arena_at(frame_idx) -> *const Vec<ScriptObject>`，**镜像现有 `ctx.frame_state_at`**
  （`RefKind::Stack` 已用它跨帧访问 caller 帧 regs——**同款成熟机制**）。`obj_new`(stack)：在**当前帧**
  arena 建 `ScriptObject`、构造 `this = StackObject{frame_idx=当前帧链下标, idx}`、跑 ctor；ctor 子帧
  `FieldGet/Set` 经 `stack_arena_at(frame_idx)[idx]` 解析。数组同法（frame_idx=创建帧）。
- **arena 稳定性**：嵌套 `obj_new`（对象 A 的 ctor 里 `new B`）会增长 arena → **index-based 访问**
  （非裸指针）天然免 Vec 重分配失效；无需 Box-per-object（保住"零 GC 且零额外 malloc"）。
- **frame_idx 有效性**：栈对象不逃逸 ⇒ 不活过创建帧 ⇒ `frame_idx` 在其整个生命期都指向仍在栈上的
  创建帧（与 `RefKind::Stack` 的不 stale 论证同理）。
- **异常回退**：ctor `throw` 时，创建帧 arena 里半构造的对象需随帧 unwind 截断释放（帧退出 truncate）。
- **工作量**：= A 的全部 **＋** `ctx.stack_arena_at` 新增 + frame_idx 穿线进 `obj_new`/ctor 调用 +
  `StackObject` 双字段变体 + GC 根扫描**逐帧**扫对象 arena（vm_context.rs:660 已逐帧循环，+1 段）+
  异常 unwind 截断 + exec_object.rs 跨帧 FieldGet/Set。
- **风险**：**中–高**。引入**新跨帧引用语义** + GC 生命期 + 异常 unwind + arena 截断，任一处错 =
  **悬垂栈引用 = 内存破坏**（比数组难 debug）。且**本地无法完整验证**（见下「本地验证限制」）→ 首次
  真实验证在 CI。
- **覆盖/价值**：**高**。对象临时（struct 式 Point/tuple/小 record）是 alloc 热区主力。

#### 方案 C —— VmContext 级 arena + 内部可变（否决）

thread 级 id-arena 可跨帧，但 `ctx: &VmContext` 需 `RefCell`/锁做内部可变 → 在**热分配 + 字段写**路径
上加 borrow/锁开销，**与本优化「去锁去 GC」的性能初衷相悖**。且 re-entrant `exec` 下 RefCell 借用易 panic。**否决。**

**推荐**：**方案 A（数组优先）**——低风险、precedent-aligned、可端到端验证（种子刷新后/CI）；对象作
follow-up 走 **B2**（index-based 跨帧），待有匹配种子能本地验证时再落。**编译器分析 + IR 标志已同时覆盖
对象与数组，故对象 follow-up 是纯运行时增量**（零编译器/格式改动）。

> **User 裁决：B2（对象+数组同批）。实施采用「per-context arena」realization（比上面 frame_idx 草图更简更安全）**：
> 实施勘探发现，per-frame arena + `frame_idx` 跨帧句柄会 ripple 全部 14 处 `VmFrame::new` + 裸跨帧指针，
> 风险高且本地不可验。改用 **per-thread（per-`VmContext`）arena + LIFO 截断**：
> - 句柄 = `Value::StackObject { idx, frame_id }`（8B，无 frame_idx）；arena 在 `VmContext::stack_arena`
>   （`Mutex<StackArena>`，owner 无竞争、GC 扫描在 safepoint）。**ctor 子帧天然可解**（同 `ctx`，无跨帧机制）。
> - 生命期：帧入栈时 `push_frame` 记录 arena 长度基线（`VmFrame::stack_obj_base/arr_base`）；`pop_frame`
>   截断回基线，LIFO bulk-free 该帧的栈分配。嵌套（对象 ctor 里再 `new`）自然 LIFO 正确。
> - 全安全 Rust（`Mutex`+`Vec`+索引，无裸指针）→ 大幅降低盲写风险。`interp/stack_alloc.rs` 自包含 +
>   8 个 Rust 单测（含 frame_id staleness / 复用槽拒旧句柄 / 截断失效 / LIFO 嵌套）本地全绿。
> - 与 D7 诊断结合：`frame_id` 校验 = 悬垂句柄的核心防线（帧退出截断后旧句柄 idx 越界 / 或复用槽
>   frame_id 不符 → 明确报错）。

> **价值权衡的诚实提示**：若 User 更看重「捕获对象临时（更常见的 alloc 热区）」而愿担 B 的风险/CI-only
> 验证，选 B2；若优先「先安全落地可测的一块」，选 A。两者编译器侧完全相同，差异纯在 interp 运行时。

### Decision 7: 诊断 —— 栈分配出错要能第一时间知道（User 要求，2026-08-05）

**问题**：栈分配的错误模式是**悬垂栈引用**（逃逸分析误判 → 栈对象/数组的引用逃出创建帧 → 帧退出后被
读 = use-after-free）。这类 bug **静默时就是内存破坏，极难定位**。要求：出错要有**清晰信号**而非 UB。

**多层防线（适用于 A / B 任一方案）**：

1. **epoch 中毒（stale-handle 捕获，核心防线）**：帧 arena 条目带一个 `epoch`；帧退出时 arena
   truncate 并**bump 一个帧级 epoch 计数**。栈句柄（`StackArray`/`StackObject`）携带创建时的 epoch；
   每次解引用**校验 epoch + idx 越界**，不符 → **明确 panic**：`stack-alloc handle used after its
   creating frame exited — escape analysis miscompiled <fn>@<pc>`。把「悬垂访问」从静默 UB 变成**指名
   道姓的诊断**。（B 方案 frame_idx 也一并校验：该帧还在链上且 epoch 匹配。）

2. **逃逸汇点运行时反查（debug 断言，静态分析的动态交叉验证）**：逃逸分析**声称**栈句柄永不到达的那些
   汇点——`is_heap_ref`/写屏障（存入堆 slot）、`RetTerm` 物化、传参给 call、`ArraySet.val` 存堆数组——
   在 **debug build 加断言**：若发现 `StackObject`/`StackArray` 流到这些点 → panic `stack handle
   reached escape sink <kind> — escape analysis unsound`。**等于用运行时证据反证静态分析**，第一时间抓到
   规则表漏判。

3. **GC 遍历校验**：mark 阶段若在**堆对象的 slot**里发现栈句柄（本不该出现）→ 诊断（同 2，GC 侧兜底）。

4. **越界永远显式**：arena 索引访问一律 bounds-check → 越界 → 明确错误，绝不裸 UB。

5. **运行时一键关优化（免重编 triage）**：环境变量 `Z42_STACKALLOC=off` → interp 忽略所有 stack_alloc
   标志、全部堆分配。用途：疑似栈分配引发的 bug，一个开关**不重编**即可二分定位「是不是栈分配的锅」
   （编译期 `--no-opt stack-alloc` 需重编，这个是运行期旁路）。

6. **可观测计数（可选）**：`Z42_STACKALLOC=stats` 打印本次运行栈分配命中数 / 各类型 / 逃逸回退数，
   便于确认优化确实生效 + 覆盖面。

> 防线 1+2 是**核心**：它们把这个优化最危险的失败模式（悬垂引用）从「静默腐败」转成「带函数名/汇点
> 名的 panic」，满足「弄错了能第一时间知道」。5 是**triage 加速器**（生产/CI 出问题时先旁路再定位）。

### 本地验证限制（2026-08-05 实施期发现，informational）

格式 bump（zbc 1.29/zpkg 0.34）后**新 VM 读不了 0.33 种子**，两代自举需**旧种子 VM**；而本仓固定的
`.z42` 种子（Aug 3）**在 builtin 上已落后当前 main**（旧 VM 跑当前 stdlib 触发 `unknown builtin
__str_to_chars` panic）→ **本地无法完整跑通编译器自举**。故：

- **Rust 运行时（Phase 1 + Phase 3）**：`cargo build` + `cargo test --lib` **本地可完整验证**。
- **编译器 pass（Phase 2）自举字节/e2e**：**以 CI 为准**（[bootstrap-seed.md] cold 路径约定），或本地
  先 `install-z42` 刷新到匹配的新种子再验。已本地确认的：**种子 z42c 能编当前 z42.ir 改动**（step 1
  全 24 库重建成功）；`IrEscapeAnalysis` 的自举编译验证待种子刷新 / CI。

## Implementation Notes

### 逃逸汇点规则表（可扩展核心 —— 满足"后面可补规则"）

单一引擎 `_computeEscapedRegs(fn) -> bool[maxReg]`：一趟扫全函数，把每个"经逃逸角色读到的 reg"入种子集，
再对 copy 传递闭包（`dst=copy src`：dst 逃逸 ⇒ src 逃逸）。规则表 = 两个角色分类器，**加精度 = 往表里
加/改分支，引擎不动**：

| 指令 / 终结子 | 逃逸角色操作数（入种子） | 中性角色操作数（不入） |
|---|---|---|
| `RetTerm` | 返回值 reg | — |
| `ThrowTerm` | 异常 reg | — |
| `FieldSetInstr` | **value** reg | target(receiver) reg |
| `FieldGetInstr` | — | target reg（读自身字段不泄露对象） |
| `ArraySetInstr` | **value** reg | array reg, index reg |
| `ArrayGetInstr` / `ArrayLenInstr` | — | array reg, index reg |
| `CallInstr` / `VCallInstr` / `StaticCallInstr` | **所有 args + receiver** | — |
| `StaticSetInstr` | value reg | — |
| `MkClosInstr` | 所有捕获 reg | — |
| 算术/比较/位/一元/convert/const/copy | — | 全部读操作数中性（copy 走传递闭包） |
| **规则表未列的任何指令** | **其所有读操作数（保守兜底）** | — |

> **铁律（对齐 LICM 的保守姿态）**：规则表**不认识**的指令读了目标 reg → **默认判逃逸**。新增指令类型时
> 若未同步登记 → 保守判逃逸（至多少优化、绝不误判不逃逸 = 绝不产生悬垂栈引用）。这是安全兜底，`IsPure`
> 白名单同款思路。

`ObjNew` 结果的 ctor 摘要检查：`_ctorLeaksThis(m, ctorName)` = 在 m 中按 `ctorName` 找到 ctor `IrFunction`，
对其跑 `_computeEscapedRegs`，返回 param-0 reg 是否在逃逸集（找不到 ctor / 跨包不可见 → 保守 true=逃逸）。
按 ctorName 缓存。

### pass 主流程（模块级）

```
IrEscapeAnalysis.Run(IrModule m):
  ctorCache = {}                                  // ctorName -> bool(this逃逸)
  for f in m.Functions:
    escaped = _computeEscapedRegs(f)              // bool[f.MaxReg+1]
    for each instr in f:
      if instr is ArrayNewInstr | ArrayNewLitInstr:
        r = instr.Dst.Id
        if !escaped[r] && _singleDef(f, r):  instr.StackAlloc = true
      if instr is ObjNewInstr:
        r = instr.Dst.Id
        if !escaped[r] && _singleDef(f, r)
           && !_ctorLeaksThis(m, instr.CtorName, ctorCache):  instr.StackAlloc = true
```

- `_singleDef(f, r)`：复用 `_computeDefs` 范式确认 `defs[r]==1`（单赋值 temp；命名局部多定义 → 不标，保守）。
- **确定性**：按声明序 / 块序 / 指令序顺扫，输出确定 → 自举字节不动点前提（见 D7 类比）。

### IR 指令字段（照 MkClos 先例）

`IrInstr.z42`：`ObjNewInstr` / `ArrayNewInstr` / `ArrayNewLitInstr` 各加 `public bool StackAlloc = false;`
（默认 false = 堆，向前兼容 pass 不跑时的行为）。`ZbcInstr.z42` 各在原编码尾部加
`if (x.StackAlloc) w.WriteU8(1); else w.WriteU8(0);`（镜像 MkClos:137）。

### 运行时 arena（照 env_arena 先例）

- `Frame`（interp/mod.rs）加 `pub stack_obj_arena: Vec<StackObjectData>`、`pub stack_arr_arena: Vec<StackArrayData>`
  （或统一 `Vec<Box<[Value]>>` + 类型标签；实现细节，最终以最简为准）。初始化处（241/265/294）置空。
- `Value::StackObject(Box<StackObjectData>)` / `Value::StackArray(Box<StackArrayData>)`（types.rs，新 tag，
  维持 24B 布局约束——payload 装箱，如 StackClosure）。`StackObjectData { type_desc: Arc<TypeDesc>, arena_idx: u32 }`
  （slots 在帧 arena；Value 只携 idx + type_desc 句柄，类比 StackClosureData 携 idx + fn_name）。
- `exec_object::obj_new`：flag=true → 构造 slots push 进 `frame.stack_obj_arena`、返回 `Value::StackObject{idx}`；
  ctor 照常在该 arena 对象上跑（ctor 只做字段初始化，已由 D3 摘要保证不外泄 this）。flag=false → 原堆路径。
- `FieldGet/FieldSet`（exec_object/exec_instr）：`match value { Object(gc)=>heap slots; StackObject(sd)=>frame.stack_obj_arena[sd.idx].slots }`。
- 数组 get/set/len（exec_array/exec_instr）同构处理 `StackArray`。
- **GC 子引用遍历**（types.rs:874/908 + arc_heap.rs:1746 `size_of` 分支）：`StackObject/StackArray` 自身
  **不产生可回收子引用**（对象本身不在堆），**但其 slots 里的堆 GcRef 必须被标记**——这由**根扫描器**负责
  （下条），子引用遍历里把栈变体归入"yield no children"（types.rs:874）即可。
- **GC 根扫描**（vm_context.rs:660-664 外部根扫描器）：现遍历 `frame.regs` + `frame.env_arena`；**加遍历
  `frame.stack_obj_arena` / `frame.stack_arr_arena` 的每个 slot Value 作根 `visit(v)`**。栈对象的堆字段引用
  由此进 mark queue，不被误 sweep。

### 版本 bump（version-bumping.md 第 5 步 checklist）

- `ZbcFormat.z42` `Minor` 28→29；`zbc_reader.rs` `ZBC_VERSION` 1.28→1.29、`ZPKG_VERSION` 0.33→0.34
  （加 wire 字段耦合规则连带 bump zpkg）。
- zbc-format / zpkg-format fixture 重生；z42c golden hex 单测更新。
- **bootstrap-seed 纪律核对**：本变更改 zbc/zpkg 格式 = 格式维度演进。**support 先行**——z42c 加"能写/能读
  StackAlloc 字段"的能力，但**当次自建的 z42c 源码 + stdlib 不依赖比上一 nightly 更新的语法**（本变更不加
  新语法，只加 IR 字段 + 格式 minor）。格式 bump 的两代自举由 `fix-bootstrap-format-bump-deadlock` 机制
  CI 自动兜底（见 bootstrap-seed.md 轴④/格式漂移）。**不要与其它格式 bump 踩同一 nightly**。

## Testing Strategy

- **pass 单测**（`z42c.semantics/tests/escape_analysis/`）：① 非逃逸对象/数组 → StackAlloc=true；
  ② 各逃逸汇点各一例（返回/字段存值/数组存值/传参/传 receiver/throw/静态存/闭包捕获）→ false；
  ③ ctor 泄漏 this（存静态/传出）→ 对象 false；ctor 纯字段初始化 → true；④ copy 传递逃逸；
  ⑤ 多定义命名局部 → 保守 false；⑥ 规则表未知指令读 → 保守 false；⑦ `Opt.StackAlloc` 单独开逐字节确定。
- **golden e2e**（`src/tests/`）：非逃逸对象/数组的字段读写、循环创建、含堆字段（String）——interp 与 JIT
  输出逐字节一致（JIT 堆分配 / interp 栈分配，输出等价）；逃逸对象仍堆分配、行为不变。
- **GC 压测**：栈对象持堆字段引用，触发 GC，确认堆字段不被误回收（根扫描器覆盖）。
- **GREEN**：`xtask test` 全 stage；`cargo test --lib`（[[xtask-test-excludes-cargo-test]]：runtime 改动
  必须补跑 Rust 单测）；e2e-direct interp+jit 双跑全 flat 语料（[[perf-alloc-and-array-levers]] 流程）。
- **自举不动点**：pass 改 codegen 输出 → 引入当次 gen1≠gen2 破一代（D7），重建 gen2==gen3 自愈；`xtask
  test compiler` 跑两遍 5/5。

## Deferred / Future Work

### escape-stack-future-jit-arena
- **来源**：本 change design D2。**触发原因**：v1 interp-first，JIT 内联数组快路与栈变体冲突面大。
- **前置依赖**：JIT helper + translate arena 落地 + JIT 字段访问识别栈变体。**触发条件**：JIT alloc 成为
  bench 瓶颈且 interp 侧已验证收益。

### escape-stack-future-interproc-summary
- **来源**：design D3。**触发原因**：v1 ctor 摘要不递归、方法调用一律判逃逸，合格面窄。
- **前置依赖**：模块级参数逃逸摘要不动点。**触发条件**：合格率不足、方法调用后仍不逃逸的对象成主流。
- **当前 workaround**：ctor 内联（IrInline）可让部分 ctor 的 this 流在调用方直接可见。

### escape-stack-future-scalar-replacement
- **来源**：design D1 选项 B。**触发原因**：arena 仍构造 Value + 跑 ctor，标量替换可彻底消除。
- **前置依赖**：ctor 内联 + 字段→寄存器重写。**触发条件**：栈对象仍占 dispatch/内存热点。

### escape-stack-future-scope-arena-reset
- **来源**：design D1 备注。**触发原因**：v1 arena 随帧退出释放，热循环内每次创建累积（同 StackClosure）。
- **前置依赖**：作用域/回边级 arena high-water 复位。**触发条件**：长循环栈分配内存增长成问题。
