# Design: `switch` 表达式无匹配臂时抛异常

## Architecture

```
OperatorEmitter._emitSwitchExpr（:149-186）
────────────────────────────────────────────────────────────────────────
  subj   = Emit(sw.Subject)
  result = Alloc(ToIrType(sw.Type()))            ← 只在某个臂采纳时才被写
  loop 逐臂：
    ├─ 无 pattern（default / `_ =>`）→ 写 result; Br(endL); ai = ArmCount   ← 提前结束
    └─ 有 pattern → EmitMatch(thenL, nextL); [守卫]; 写 result; Br(endL); StartBlock(nextL)
  ────────────────────────────────────────────────────────────────────
  if (!Ended) { Br(endL) }        ← ★ 落空点：只有「没有兜底臂」时才到得了
  StartBlock(endL); return result
```

`★` 处当前直接 `Br(endL)`，`result` 一次都没写 ⇒ 读未初始化槽位。改为在此发射
**new + throw**，`endL` 只保留「某臂采纳」这一类入边。

改后的落空块：

```
ConstStr  msg0, "switch expression did not match any arm; value: "
<三分支> vs = ToStr(subj)        // 复用插值的 hole 逻辑，见 D3
StrConcat msg, msg0, vs
ObjNew    exc, "Std.SwitchExpressionException", "Std.SwitchExpressionException.SwitchExpressionException$1", [msg]
Throw     exc                     ← 终结符，块自然 Ended
```

`Throw` 使 `_ctx.Ended` 为真，所以**不再走 `Br(endL)`**——这正是让 `result` 不再被
未初始化读的机制：落空路径根本到不了 `endL`。

`endL` 在「一个臂都没有」这种极端形态下会变成无入边死块。`IrDeadBranch._removeUnreachable`
会清掉它（后继枚举只认 Br/BrCond，`Throw` 计 `NoBranch`）。

---

## Decisions

### D1：抛，而不是产默认值 / 不是把 W0700 升级成 error

User 已裁决「运行期抛，对齐 C#」。留档三个选项与被否的理由：

| | 做法 | 为什么不选 |
|---|---|---|
| A | 落空产该类型的默认值（`0`/`false`/`null`） | 把垃圾值变成**确定的**错值。仍是静默语义缺口——错误照样往下游流，只是可预测了 |
| B | 把 `W0700` 对表达式形态升级成 error | **覆盖不到问题**：`W0700` 只查 bool / enum（`ExhaustCheck.z42:5-10`），三个复现的 subject 都是 `int`（开放域）⇒ 一条诊断都没有。且会把今天能编的 bool/enum switch 表达式变成编译错误 |
| **C（选定）** | 运行期抛 `SwitchExpressionException` | 唯一覆盖开放域的方案；C# 同语义 |

> A 与 C 不冲突但不叠加：既然 C 已把静默性彻底消除，再产默认值只会掩盖。

### D2：编译期合成 new + throw，**不新增 IR 指令**

| | 做法 | 代价 |
|---|---|---|
| A | 新增「抛内置异常」opcode，VM 侧用 `make_stdlib_exception` 构造 | **zbc minor bump ⇒ 连带 zpkg minor bump**：`version-bumping.md` 9 步 + 6 个 zbc / 4 个 zpkg 字节基线重生 + `refresh-format-fixtures` 门 + 触发 ci-bootstrap 两代自举；且 0x40–0x44 终结符段已用满，要占 0x45 |
| B | emitter 发一条到 stdlib thrower helper 的 `CallInstr`，helper 里 `throw` | 零格式变更；但多一个 stdlib 方法、栈回溯多一帧 |
| **C（选定）** | emitter 直接合成 `ObjNew` + `ThrowTerm` | 零格式变更、零 VM 改动、零新增 stdlib 方法；与用户写 `throw new X(...)` 发出的指令**完全同形** |

C 之所以可行，是因为 `ObjNewInstr` 的类名与 ctor 名**本就是裸字符串字段**
（`IrInstrObject.z42:52-54`），编译期不要求符号表命中（解析失败在运行期抛可 catch 的
`MissingSymbolException`）。先例：`RecordSynth.z42:232-262` 已在合成 `ConstStr` + 多块控制流。

> **B 的唯一优势是保住内联**：`IrInline._termInlinable`（`:316`）白名单只有
> `Ret|Br|BrCond` ⇒ 含 `ThrowTerm` 的函数整个不可内联；`IrPureFunctionTable._isFuncPure`
> （`:62`）遇 `ThrowTerm` 判非纯。但**产品代码里不穷尽 switch 表达式 0 个**（实测），
> 所以这个代价在本仓不发生；用户代码里会发生，记进 reference 的「性能注意」。
> 用 B 去换这点、代价是永久多一个 stdlib API 面 + 每条栈回溯多一帧，不值。

### D3：`ToStr` 的三分支抽成共用函数，不复制

插值的 hole 已经把「怎么把任意值变成字符串」处理对了（`ExprEmitter.z42:296-317`）：

```
enum        → _boxEnumForStr（否则打出序号而非成员名）
blob struct → _emitStructToStr（ToStrInstr 走 value_to_str，没 ctx、解不了 arena blob，
                                只会吐 `<struct value>` 占位符）
其余        → ToStrInstr
```

**抽出 `internal TypedReg ExprEmitter._emitToStr(TypedReg reg, Z42Type t)`**，插值与本变更
同调一处。理由：这三分支的每一支都有血的教训（注释里写着
`fix-struct-tostr-in-interp` / `make-enum-distinct-type`），复制一份必然漂移。

### D4：消息不含源码位置

`Span.File` 在发射期是**构建机上的路径**。嵌进消息 ⇒ 路径进 zbc 字符串池 ⇒
同一份源码在不同机器/目录编出不同字节 ⇒ 破坏字节不动点与可复现构建。

位置本来就免费：`Terminator::Throw` 运行期做 `resolve_line` + `update_top_frame_pos` +
`populate_stack_trace`（`interp/mod.rs:336-345`；JIT 侧 `jit_throw`
`jit/helpers/control.rs:16-30` 同）⇒ 抛出点的函数名与行号自动在栈回溯里。

### D5：`SwitchExpressionException` 是新类，且**不需要两-nightly**

`src/libraries/z42.core/src/Exceptions/` 下按同形补一个（最近的同形先例：
`InvalidCastException.z42`，#746）。

**为什么不踩 `bootstrap-seed.md` 的「support 先行、晚一 nightly 再 use」**：

- 编译器**源码**不 `using`、不引用这个类 —— emitter 只往 IR 里写**字符串** `"Std.SwitchExpressionException"`。
  ⇒ 种子 z42c + 种子 stdlib 编当前 z42c 源时，这个类是否存在完全无关（轴 ③ 不触发）。
- 冷启动链上真正会执行合成 throw 的，是**已经用当前源建好的** z42c 编**当前** stdlib/用户码，
  那时 `Std.SwitchExpressionException` 已在新 z42.core 里。
- 退化情形（旧 stdlib + 新发码，只可能出现在两代自举的中间态）：落空时解析不到 ctor ⇒
  抛 `MissingSymbolException` 而非 `SwitchExpressionException`。**仅影响错误路径**，
  构建成功路径一步都不走它。

### D6：`CompilerFingerprint` 8 → 9

`CacheStore.CompilerFingerprint` 是**缓存失效 pin**（`CacheStore.z42:6`）：codegen 变了、
而源码哈希没变的文件，靠它作废旧条目。含不穷尽 switch 表达式的用户文件正是这种情形。
不 bump ⇒ 复用旧编译器产出的 buggy 缓存。

---

## Risks

| 风险 | 缓解 |
|---|---|
| `ObjNew.CtorKnown` 在合成站点没置上 ⇒ ctor 不被调 ⇒ `Message` 为空 | spec 有一条「消息非空且含落空值」的 e2e 钉死（proposal Q1） |
| e2e golden 里 5 个不穷尽站点的落空路径**真被执行** ⇒ 原本静默的用例现在抛 | 逐个跑；真被执行的说明该用例一直在验一个垃圾值，改成验异常（这正是想要的） |
| `examples` 两例 + 第 17 章把「静默产 null」当规则写着 | 同一 PR 内改写；`xtask test examples` 逐条实跑会立刻判红，漏不掉 |
| JIT 与 interp 行为分叉 | `ThrowTerm` 两侧都已实现（`jit/translate/term.rs:81-100`），不是新路径；e2e 仍双模式跑 |
| 落空块里 `result` 仍被 `endL` 之后读 | 落空路径 `Ended` 后到不了 `endL`；`endL` 的每条入边都写过 `result` |
