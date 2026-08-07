# Design: sealed 修饰符语义强制 + 元数据 + 反射

> **拆分（2026-08-07）**：④ 去虚化拆为 follow-up `add-sealed-devirt`（详见 proposal 拆分说明）。
> 下文 Decision 1/2 关于去虚化"定位=解锁内联"与"安全规则 v1=仅按 sealed 类"的论证**保留**——
> 它们是 follow-up 的设计依据；本 change 只落其地基（sealed 位 + 本地/跨包 `IsSealed`）。
> 本 change 落地的是 Decision 3（shorthand）/ Decision 4（格式 bump）/ Decision 5（两阶段）。

## Architecture

```
                       ┌─────────────────────────────────────────────┐
 源码 sealed / sealed override                                        │
   │  (parser 已收进 Mods，无需改语法)                                  │
   ▼                                                                   │
 SymbolCollector._passFixupOverrides                                   │
   ├─ ① 继承强制：基类 IsSealed → error                                │
   ├─ ① override 强制：基槽方法 sealed → error                         │
   ├─ ③ shorthand：Mods 含 "sealed" 无 "override" → 当作 override，     │
   │      并校验"匹配基类 virtual"（同一槽解析逻辑复用）                 │
   └─ 标 Z42ClassType.IsSealed / 方法 IsSealed                         │
   ▼                                                    ▼              │
 IrGenFacts._methodFlags                    ExprEmitter._emitCall (④)  │
   └─ Mods 含 sealed → bit2                    └─ recv.Type() 是 sealed 类
   ▼                                              → CallInstr 直接调用   │
 ZbcFormat.Minor 30 / ZpkgWriter 35              （否则 VCallInstr）     │
   ▼                                              ▼                     │
 .zbc/.zpkg (method_flags bit2)              IrInline 内联该 Call        │
   ▼                                                                   │
 zbc_reader.rs (MINOR 30/35) → bytecode.METHOD_FLAG_SEALED             │
   └─ reflection.__method_is_sealed ─────────────────────────────────┘

 跨包：ImportedSymbolLoader 从 CLASS_FLAG_SEALED / METHOD_FLAG_SEALED
       还原 IsSealed → 供 ① 强制 与 ④ 去虚化对导入类型也生效
```

## Decisions

### Decision 1: 去虚化的定位——解锁内联，而非派发提速

**问题：** "sealed 去虚化对函数调用有帮助" 的收益具体是什么？

**事实：** 解释器已有多态内联缓存 `VCallIC`（`exec_vcall.rs`，4-slot PIC，object+primitive 均缓存，#129/#131 已优化）。单态 sealed 调用点运行时已是 "IC 命中→直接 fn_idx"，仅一次 TypeId 比较。

**决定：** 去虚化的目标定为**编译期 `VCall`→直接 `Call`，从而让 `IrInline`（#102）能内联 virtual 方法**（VCall 无法被 inline pass 消费）。派发提速交给既有 PIC，不重复投入。这也让本特性干净接入现有优化管线（copy-prop / CSE / LICM / inline / loop-alloc-reuse），而非在运行时另造机制。

### Decision 2: 去虚化的安全规则 v1——仅按 receiver 静态类型是 sealed 类

**问题：** 什么情况下 `VCall` 可证明单态、安全降级？

**选项：**
- A — receiver 静态类型是 sealed 类：该类不可被继承 → 其上任何虚方法解析唯一 → 降级安全。**最保守可靠。**
- B — 方法是 `sealed override` 且 receiver 是任意类型：不安全，receiver 若是基类型，实际对象可能是别的 override（sealed 只保证"到此为止不再 override"，不保证 receiver 就是这一层）。
- C — 全程序单实现分析（某 virtual 全程序只有一个 override）：需闭世界假设，跨包不成立。

**决定：** v1 选 A。B 记入 Deferred（需精确类型分析）。C 不适合开放包世界。
**理由：** A 的可靠性来自"sealed 类不可继承"这一**局部、跨包也成立**的事实（`CLASS_FLAG_SEALED` 导入可见），无需全程序分析，且 `c.Receiver.Type()` 在 `_emitCall` 处已可得。

### Decision 3: 方法 `sealed` 作 `sealed override` 简写——两种写法都收

**问题：** 是否允许方法上单写 `sealed`（省略 override）？

**决定：** 允许，且**仍接受 `sealed override`**。二者语义等价：方法级 sealed **必须**解析到基类某 virtual（否则 error）。
**理由：**
- z42 方法默认非虚，非 override 方法本就不可 override，对其 sealed 无意义——故方法级 sealed ⟺ sealed override，`override` 可从 sealed 推出。
- "必须匹配基类 virtual" 的校验本就是 ① override 强制所需，shorthand 复用同一槽解析逻辑，**不增净复杂度**。
- 仍收 `sealed override` → C# 代码可原样粘贴；z42 原生代码可更短。代价仅"同一语义两种拼法"，换 C# 兼容 + 简化，值。
- **纯 semantics**：parser 早已把 `sealed` 收进 `Mods`，无语法改动。

### Decision 4: 加 method_flags sealed 位需要 zbc 格式 bump（尽管字节布局不变）

**问题：** `method_flags` 已是 u8（1.24 起），bit2 先前保留为 0。仅赋予 bit2 语义、不改字节宽度，是否要 bump minor？

**决定：** **bump（zbc 1.29→1.30，联动 zpkg 34→35）。**
**理由：** strict-pin 的目的正是防止"同版本号、不同语义解释"的静默分歧。若不 bump，一个"旧 1.29 VM（不识 bit2）"与"新 1.29 VM（识 bit2）"会互相接受对方的 `.zbc` 却对 bit2 解释相左——跨 nightly 种子链上即为静默错读。bump 强制该不匹配显式暴露。符合既有先例（add-method-modifiers=1.24、add-reflection-type-flags=1.12 均为"定义字段/语义新增"即 bump）。
**代价（已随全量 scope 接受）：** 触发 `bootstrap-seed.md` 两阶段 support/use 纪律 + fixture 重生。**注意本 change 不在 z42c/stdlib 源码写 sealed**，故 golden fixture 的变化仅来自 header minor 字段（bit2 实际全 0，方法体字节不变）。

### Decision 5: 两阶段落地（support 先行）

**问题：** 格式 bump + 新语义如何不断自举链？

**决定：** 本 change 只落"支持"：
- z42c 加 sealed 全套能力（①②③④）、writer 产 1.30 格式；
- **z42c / stdlib / xtask 源码本 change 不使用 `sealed`/`sealed override`**（上一 nightly 种子仍能编当前源）；
- examples / tests 由**当前构建的 z42c** 编译（非种子路径），本 change 即可用 sealed 验证 ①②③④。
- 下一 nightly 发布后，另开 change 在编译器/库源码 use sealed。
**理由：** 遵 `bootstrap-seed.md` "support 先行、use 晚一 release"；格式 bump 的 CI 死结已由 `fix-bootstrap-format-bump-deadlock` 的两代自举根治，build-and-test 路径自动过。

## Implementation Notes

- **override 槽解析复用**：③ 的 "sealed 必须匹配基类 virtual" 与 ① 的 override 强制，都落在 `SymbolCollector._passFixupOverrides`（:252）已有的"按签名找基类槽"逻辑上——找到槽=合法 override；找不到 + 有 sealed → error；找到的槽标了 sealed → 子类再 override 报错。
- **`Z42ClassType.IsSealed` 来源**：本地类从 `ClassDecl.Mods` 含 sealed；导入类从 `ImportedSymbolLoader` 读 `CLASS_FLAG_SEALED`。方法 sealed 同理（本地 Mods / 导入 `METHOD_FLAG_SEALED`）。
- **④ 降级点**：`ExprEmitter._emitCall`（:651）现由 `EmitContext.ReceiverMethodIsVirtual(c.Receiver.Type(), c.MethodName)`（:678）决定 VCall。新增：若 `_receiverClassType(c.Receiver.Type())` 非空且 `IsSealed` 且能解析到唯一实现 FQ 名 → 发 `CallInstr`。保留所有既有守卫（cast-to-class Unknown 链、接口 receiver 恒 VCall 等）——它们优先于去虚化（Unknown 链上不去虚化）。
- **`_methodFlags`（IrGenFacts:62）**：在现有 virtual/abstract 位判定旁加 `if (_hasWord(mods,"sealed")) flags |= 4;`。注意 shorthand 情形下，SymbolCollector 需保证方法 `Mods` 在语义上等价含 override——但 `_methodFlags` 只关心 sealed 位本身，virtual 位由 override/virtual 触发（shorthand 已被解析为 override，故 virtual 位也正确置上）。
- **version bump 文件集**：严格按 `.claude/rules/version-bumping.md` 的 zbc(1–5) + zpkg(6–9) 九步，勿漏 golden hex 重截与 fixture 重生。

## Testing Strategy

- **单元（z42c semantics，编译期报错）**：继承 sealed 类 / override sealed 方法 / sealed 无匹配 virtual —— 均须报对应错误；shorthand `sealed` 与 `sealed override` 编译等价。
- **Golden / e2e（去虚化正确性）**：sealed receiver 调用输出与虚派发逐字节一致；构造"若调错实现则输出不同"的用例，确保降级选对实现。
- **反射**：`MethodInfo.IsSealed` true/false。
- **格式**：`cargo test --test zbc_compat` / `cargo test lazy_loader` 读新基线；`xtask test compiler` golden hex 绿。
- **VM 验证**：完整 `xtask test`（全 stage gate）。去虚化改 IR 输出 → 当次 gen1≠gen2 破一代自举（D7），按既有机制重建自愈。
- **JIT**：interp 全绿前不动（去虚化在编译期已生效，JIT 消费同一 IR，天然受益；不额外为 JIT 写去虚化逻辑）。
