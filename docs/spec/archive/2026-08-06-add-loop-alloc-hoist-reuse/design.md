# Design: 循环内分配 hoist + 对象复用

## Architecture

```
IrOptPipeline.Run(m, optSet)
  ├─ 每函数：const-fold / copy-prop / DCE / inline / CSE / LICM
  ├─ IrEscapeAnalysis.Run(m)          ← 已有：标 ObjNew/ArrayNew.StackAlloc（非逃逸）
  └─ IrLoopAllocReuse.Run(m)          ← 新：需 StackAlloc 结果，故排在其后
       每函数 f：
         建 CFG + 支配 + 自然循环（复用 IrLicm._computeDom / _loopBody / 干净 pre-header 判定）
         对每个「自然循环 + 干净 pre-header」：
           扫循环体找候选分配指令（ObjNew / ArrayNew）
           对每个候选 alloc（dst=%r）判 5 条资格（见 Decisions D2）
           命中 → 变换（见 D3）：分配移 pre-header + 循环体改 reinit
```

变换后 IR 对 interp 和 JIT 都生效（同一份 IR）。运行时仅需保证「空 ctor 名 ObjNew = 裸分配」。

## Decisions

### D1: 复用 IrLicm 的循环机件，不重造 CFG

**问题**：hoist 需要 CFG / 支配 / 自然循环体 / 干净 pre-header——LICM 已全有。
**决定**：`IrLoopAllocReuse` 复用 `IrLicm._computeDom` / `_loopBody` / 干净 pre-header 判定（header 唯一
循环外前驱 `ph` 且 `ph` 终结子 `br h`）。**把这些从 `IrLicm` 的 private 提为 internal/复制到共享 helper**
（择一，实施时定；倾向抽到 `IrLoopUtil.z42` 供两 pass 共用，避免 LICM 私有耦合）。无干净 pre-header 的循环
**跳过**（不做 CFG 手术），与 LICM 同保守策略。

### D2: 资格判定 —— 5 条全满足才 hoist（任一不满足则跳过，安全兜底）

候选分配 `%r = alloc(...)` 在自然循环体内，须全满足：

- **C1 非逃逸**：`alloc.StackAlloc == true`（逃逸分析已证不逃逸函数）。可复用 ⟹ 不逃逸函数
  （逃逸者被存到持久处，复用会让逃逸引用看到改写），故 C1 是必要前提，直接复用已有结果。
- **C2 迭代内局部（不跨迭代携带）**：`%r` 及其存入的 slot 在**回边处已死**（不 live-out of latch）。
  v1 保守实现：`%r` 的所有使用点在循环体内，且 `%r` 存入的每个 local slot 在循环 header 处**非 live-in
  （来自回边）**——即每迭代进入时该 slot 被本迭代的分配重定义，上一迭代的值不被本迭代读。用**回边 slot
  活跃性**（后向数据流，新增轻量分析）判定。`prev = p`（循环携带）→ slot_prev live-across-回边 → C2 失败 → 跳过。
- **C3 形状固定**：`ArrayNew` 的 `Size` 循环不变（定义在循环外 / 常量）；`ObjNew` 类固定（恒真）。
- **C4 重初始化完整**（复用后无脏值残留）：
  - **对象**：ctor this-safe（复用 `IrEscapeAnalysis._ctorLeaksThis == false`）**且** ctor 字段写无条件
    （v1：ctor 体单基本块 / 所有 `FieldSet(this)` 支配 ctor 出口）。→ 每迭代重跑 ctor 覆写全部被写字段，
    未被写字段保持裸分配的零初始化（与 fresh 分配一致）。
  - **数组**：读前必写全（v1：所有 `[0,Size)` 下标以**常量下标**在任何读之前写全；证不到 → 跳过）。
- **C5 在干净 pre-header 的自然循环**（D1）。

### D3: 变换机制 —— 无格式 bump，无新 IR 指令

**对象**（`%r = ObjNew(Cls, ctor, [args])` 在循环体）：
1. pre-header 追加：`%r = ObjNew(Cls, ctor="", [])`（**空 ctor 名哨兵 = 裸分配**；保留 `StackAlloc` 标 →
   arena 只分配一次、复用、帧退出释放）。
2. 循环体原址替换为：`Call ctor(%r, [args])`（静态调用 ctor 函数，`this=%r`——运行时 obj_new 内部本就
   `exec_function(ctor, [obj, ...args])`，此处显式化）。
3. `%r` 的其余使用点不变（同一寄存器，pre-header 定义支配全循环）。

**数组**（`%r = ArrayNew(Size, elem)`，Size 循环不变、读前写全）：
1. 整条 `ArrayNew` 移到 pre-header（它本就是裸分配，无 ctor）。保留 `StackAlloc`。
2. 循环体依赖既有元素写回重初始化（C4 已保证写全）。原址无残留指令。

**为何不加「裸分配」新 opcode / 不加 skip-ctor 位**：空 ctor 名走运行时既有 `outcome=None` 路径即得裸分配
（`func_index.get("")==None && try_lookup("")==None` → 不调 ctor）——零格式变更。符合 philosophy「最简 emit」。

### D4: 数组「读前写全」判据（v1 收紧，宁可漏不可错）

v1 只认：循环体内该数组的**所有读**（ArrayGet）之前，`[0, Size)` 每个下标都有一次**常量下标** ArraySet
（Size 为编译期常量或循环不变且能证全覆盖）。动态下标 / 部分覆盖 / Size 不可判 → **不 hoist**（回退到既有
逐迭代分配，安全）。此判据覆盖「builder 式填满小数组」的常见模式（如基准 `a[0]=;a[1]=;a[2]=`）。更强的
「循环写覆盖 + memset 兜底」留 Deferred。

### D5: 与 escape-stack-alloc / scope-reset 的关系（诚实记录成本/收益）

**实测校正（2026-08-06，实施后量测）**：初稿预测「增量 perf 收益偏小」（以为逃逸分析已栈分配 → reuse 只
省一次廉价 arena push）。**该预测过于保守、已被实测推翻**：循环体基准（`new Point`+`new int[3]` ×8M，
reuse ON vs OFF，两者 escape-stack-alloc 皆开）**interp 2.91× / jit 4.09×**（System 0.002s vs 0.22–0.30s）。
根因：低估了**帧 arena 累积**代价——OFF 每迭代往 arena 塞对象+数组、累积到 **1600 万条目**，Vec 反复扩容 +
内存/缓存压力主导运行时。这也解释了 escape-stack-alloc 单独在循环体只有 **1.01×** 的谜题：分配省下的被累积
抵消了；reuse 把 arena 清到恒 2 条目 → 真实收益爆出。**结论**：本 pass 对「循环内每迭代 new 的临时对象/
数组」是**大杠杆**（不止堆分配，栈分配的累积同样是大头）。与 deferred 的 **scope/回边 arena 复位**互补——
后者用运行时回边截断也能消累积（栈侧），但拿不到本 pass 的「堆 → 1 次分配」+ 编译期零运行时开销。

### D6: 诊断（修正——本 pass 是**纯编译期 IR 变换**，无运行时开关/断言）

> **实施期修正（2026-08-05）**：初稿设计了运行时 `Z42_LOOPHOIST=off` 旁路 + interp「reinit 前被读」
> debug 断言。实施时认清两点：① 本 pass 在**编译期**改 IR，运行时拿到的已是变换后 IR——运行时无从旁路，
> 开关只能是编译期（`--no-opt loop-alloc-reuse`）；② reinit 是**普通 ctor Call / ArraySet**，运行时没有
> 「这是 reinit」的干净钩子，且 SSA 支配已从原理上排除「reinit 前被读」（uses 被 def=ctor-Call 支配）→
> 该断言既难干净实现、又几乎不可能触发。故**删去运行时旁路 + 运行时断言**，诊断收敛为下面两条编译期手段。

- **编译期开关旁路（对拍）**：`--no-opt loop-alloc-reuse` 关本 pass → 与开启版**同一程序输出逐字节对拍**
  （e2e golden 双跑）。这是本 pass 的**主正确性门**——任何 miscompile（C1–C4 判漏）都会表现为开/关输出不一致。
- **变换隔离可见**：IR dump（`Opt.All - Opt.LoopAllocReuse`）单独看开/关本 pass 的 IR 差异，golden 稳定。
- **既有 escape 诊断仍覆盖**：hoisted 的栈对象/数组仍是 `Value::StackObject/StackArray`，运行时 frame_id
  悬垂校验 / 逃逸汇点断言（add-escape-analysis）继续对它们生效——句柄错用仍明确报错、非静默 UB。
- **编译期健壮性**：所有 reg-索引数组访问带 `< rc` 边界保护（ctor-call 的 dummy dst 会 bump MaxReg，
  超出预算 rc 的 id 被守卫跳过，绝不 OOB）。

## Implementation Notes

- **pipeline 位置**：必须在 `IrEscapeAnalysis.Run(m)` **之后**（依赖 `StackAlloc`）。
- **空 ctor 名裸分配**：实施首验 `obj_new` 的 `ctor_name==""` → `outcome=None` 不报错（若报「ctor not
  found」，收窄为「查不到 + 名为空 = 有意裸分配」分支）。
- **迭代内局部（C2）活跃性**：新增轻量后向活跃性（slot 粒度，只需判「回边处哪些 slot live」）。可局部于本
  pass，不必全函数 SSA。
- **ctor 静态调用**：用既有静态 `CallInstr`（逃逸分析规则表已列 `Call*`），callee=ctor FQ 名，args=`[%r]++原args`。
- **行数**：`IrLoopAllocReuse.z42` 控制在 300 行内；CFG/循环 helper 抽 `IrLoopUtil.z42`（若从 LICM 提取）。

## Testing Strategy

- **单元测试**（`z42c.semantics/tests/loop-alloc-reuse/`）：
  - 命中：对象 `new Point` in loop → pre-header 1 个裸 ObjNew + 循环体 1 个 ctor Call；数组同理。
  - 不命中边界：循环携带（`prev=p`）不 hoist；ctor 泄漏 this 不 hoist；动态下标数组不 hoist；Size 循环变不 hoist。
- **e2e golden**（`src/tests/run/loop-alloc-reuse-*/`）：对象 + 数组复用程序，**开/关本 pass 输出必须一致**
  （正确性），interp + JIT 双跑。
- **GREEN gate**：`xtask test`（全 stage：e2e / cross-zpkg / stdlib / compiler 自举 / vscode-syntax）。
- **回归**：自举 5/5（z42c 自身编译经本 pass，结果字节不动点或结果一致）。
