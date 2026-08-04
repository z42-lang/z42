# 逃逸分析与栈上分配

> 对齐：2026-08-05（change `add-escape-analysis-stack-alloc`）
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

**引擎 `_computeEscapedRegs(fn)`**（两趟）：
1. **Pass A**：逐指令 / 终结子，按操作数**角色**把「经逃逸角色读到的 reg」入种子集。
2. **Pass B**：copy 传递闭包不动点——`dst = copy src`，dst 逃逸 ⇒ src 逃逸（对象经 copy 流出）。

**角色感知逃逸汇点规则表（可扩展核心）**——**完整镜像 `IrOptInfo.AddReads` 的读枚举**，逐操作数分类
（漏一个 escaping 操作数 = 漏判逃逸 = 悬垂栈引用）：

| 指令 / 终结子 | 逃逸角色（入种子） | 中性角色（不标） |
|---|---|---|
| `RetTerm` / `ThrowTerm` | 返回值 / 异常 reg | — |
| `FieldSet` / `StaticSet` / `ArraySet` | **value** reg | target / array / index |
| `ArrayNewLit` | 所有 **elems** | — |
| `Call*` / `VCall` / `CallIndirect` | 所有 args + receiver + callee | — |
| `MkClos` | 所有捕获 reg | — |
| `AsCast`（结果别名）/ `ToStr`（调 ToString）/ `LoadLocalAddr`（取址）/ `IsInstance`※ | 其 Obj/Src/Slot | — |
| `FieldGet` / `ArrayGet` / `ArrayLen` | — | target / array / index |
| 算术·比较·位·移位·一元·convert·StrConcat / `ArrayNew.Size` / Copy(Pass B) | — | 全部读操作数 |
| **规则表未列的任何指令** | **其所有读操作数（保守兜底）** | — |

> ※ `IsInstance.Obj` 严格说是中性（`is`-check 不泄露引用），但**有意标为逃逸以收窄运行时触达面**——
> 如此栈对象在 interp 只经历 FieldGet/FieldSet（+ Eq 走 `Value` 的 `PartialEq`），运行时无需为
> IsInstance 补栈对象分支。代价：`p is T` 的 p 不栈分配（少量覆盖损失）。放宽 = 运行时补分支后移除该标记。

**铁律（对齐 LICM 的保守姿态）**：规则表**不认识**的指令读了目标 reg → **默认判逃逸**（over-approximate
安全兜底）。加精度 = 往规则表加/改一条分支，引擎（两趟）不动 —— 这是「后面可补规则」的落点。

**对象的 ctor this-escape 前提**：`new Foo(a,b)` 的 `ObjNew` 会带 `this` 调 ctor，若按「传给 call 即逃逸」
则所有对象都判逃逸。故对象合格需额外一条：**`_ctorLeaksThis(m, ctorName)`**——对 ctor 函数体跑同一引擎、
查 param-0(`this`，reg 0)是否逃逸（**单函数、不递归**：ctor 把 this 传给别的调用 = 保守判逃逸；跨包 /
找不到 ctor = 保守判逃逸）。缓存按 ctorName。

**对象完整合格条件**（三者皆满足）：① 结果 reg 本函数内不逃逸；② 单赋值 temp（`defs==1`）；③ ctor 不泄漏
this。数组无 ctor，只需 ①②。

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
- **生命期（LIFO 截断）**：帧入栈 `push_frame` 记录 arena 长度基线（`VmFrame::stack_obj_base/arr_base`）；
  帧退出 `pop_frame` 截断回基线，bulk-free 该帧的栈分配。嵌套（对象 ctor 里再 `new`）自然 LIFO 正确。
- **GC**：`Value` 的 `trace_children` 视栈句柄为叶；外部根扫描器在 safepoint 扫 `ctx.stack_arena` 每个栈
  对象的 slots / 栈数组的 elems 作根（它们可能持堆 GcRef，必须保活）。arena 锁从不跨 GC 触发持有 → 不死锁。
- **JIT**：读得进新 zbc 字段但**忽略**（照常堆分配）。interp-first（准则 1）：优化只服务无 Cranelift 兜底的
  interp；`interp==jit` 靠「输出相同、表示不同」成立。一个对象整个生命期在同一引擎内（tiering 以函数为粒度）
  → JIT 永不遇到栈句柄。

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
- 格式 bump：[version-bumping.md](../../../.claude/rules/version-bumping.md)
- 引入：change `add-escape-analysis-stack-alloc`（`docs/spec/`）
