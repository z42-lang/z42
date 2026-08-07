# Proposal: 基于 sealed 的去虚化（VCall→Call 解锁内联）

## Why

`impl-sealed-semantics`（#140）把 `sealed` 补成有语义的真修饰符，并落齐了去虚化地基
（`CLASS_FLAG_SEALED` + `METHOD_FLAG_SEALED` + 本地/跨包 `Z42ClassType.IsSealed` /
`MethodSymbol.IsSealed`），但**尚未消费**它做优化。

sealed 类不可被继承 → 静态类型是 sealed 类 `A` 的 receiver，其运行期实际类型**必然是 `A`**
→ `a.M()` 的目标**编译期唯一可知**。当前 z42c 对 virtual/override 方法一律发 `VCallInstr`
（运行期 vtable 派发），即使目标已确定。这带来两处浪费：

1. `IrInline`（#102）pass **吃不进 `VCall`** → sealed 类的 virtual 方法永远无法被内联。
2. 多一次 vtable 派发（虽然解释器 PIC 已把它降到近直接调用——所以**净增价值是解锁内联，
   不是派发提速**）。

不做的后果：sealed 的「编译期单态」信息白白浪费，virtual 方法在 sealed receiver 上无法内联，
错失与现有优化管线（copy-prop/CSE/LICM/inline/loop-alloc）复合的机会。

## What Changes

- **去虚化 pass**：`ExprEmitter._emitCall`（instance 分支，:730）中，当 receiver 静态类型是
  **sealed 类**且能解析到唯一目标实现时，发射**直接 `CallInstr`**（目标 FQ 名 + receiver 前置为
  arg0）替代 `VCallInstr`（:766）。发射后交由既有 `IrInline` 内联。
- **目标解析**（新增 `EmitContext` 助手）：沿 sealed 类的基链找**最近声明该方法且非 abstract**
  的类 `C`，产出 `FQ(C) + "." + RegKey`——其中 `FQ`/`RegKey` **必须逐字节匹配 IrGen 的函数命名**
  （`_q(_classIrShortName(C)) + "." + md.RegKey`，含模块 ns + 泛型 `$N` mangle）。
- **v1 安全边界**（见 design 决策）：仅 **本地非泛型 sealed 类** receiver + 本地定义类 + 非 abstract
  目标。imported sealed / 泛型 sealed / `sealed override`（receiver 为基类型）→ **Deferred**。
- 保留所有既有守卫（cast-to-class Unknown 链、接口 receiver 恒 VCall）——它们优先于去虚化。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY | 新增 `SealedReceiverClass(recvType)` 判定 + `ResolveSealedTarget(recvType, method, argCount)` 返回目标 FQ 名（沿基链解析定义类 + RegKey；不可解析/越出 v1 边界 → 返回 ""） |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `_emitCall` instance 分支：VCall fallback 前插入 devirt——目标非空 → 发直接 `CallInstr`（recv 前置）；否则原样 VCall |
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | 新增 `Opt.Devirt` 位（门控；`--no-opt devirt` 可关，供 before/after 对拍）|
| `docs/book/src/language/sealed.md` | MODIFY | 去虚化从 Deferred 段移到「机制 / 实现」，写清 v1 规则 + 与 PIC 分工 |
| `docs/design/runtime/optimization-pipeline.md` | MODIFY | 新增 devirt pass 描述（落点、解锁内联、v1 边界）|
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引登记 devirt + 关联 change |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index：sealed-devirt 从「待做」→ 落地；补 imported/泛型/sealed-override 去虚化为新 Deferred |
| `src/tests/sealed_devirt/local_sealed_declared/source.z42` | NEW | sealed 类自身声明 virtual 方法 → 去虚化 + 结果正确（配 expected_output.txt）|
| `src/tests/sealed_devirt/local_sealed_declared/expected_output.txt` | NEW | 期望输出 |
| `src/tests/sealed_devirt/local_sealed_inherited/source.z42` | NEW | sealed 类继承基类方法（不 override）→ 目标解析到基类实现 |
| `src/tests/sealed_devirt/local_sealed_inherited/expected_output.txt` | NEW | 期望输出 |
| `src/tests/sealed_devirt/nonsealed_stays_vcall/source.z42` | NEW | 非 sealed receiver → 仍虚派发（子类 override 生效，证明没误去虚化）|
| `src/tests/sealed_devirt/nonsealed_stays_vcall/expected_output.txt` | NEW | 期望输出 |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | 单测：sealed receiver 调用 emit `call @Ns.Cls.M`（非 `vcall`）；非 sealed emit `vcall`；`--no-opt devirt` 对拍 |

**只读引用**：`IrGen.z42`（`_q`/`_classIrShortName`/函数命名，目标名必须匹配）、`Z42Type.z42`
（`Z42ClassType.IsSealed`）、`Symbol.z42`（`MethodSymbol.RegKey`/`IsSealed`）、`IrInline.z42`（确认
降级后的 `Call` 被内联）、`IrOptPipeline.z42`（pass 顺序：devirt 须在 inline 前）。

## Out of Scope（→ Deferred）

- **imported sealed 类去虚化**：跨包目标名 + imported RegKey 约定更复杂（DepIndex 路径）；v1 只本地。
- **泛型 sealed 类**：`$N` mangle + 类型参数替换，目标名易错；v1 只非泛型。
- **`sealed override` 方法去虚化**（receiver 是基类型）：需精确类型/单实现分析——非「receiver 静态即
  sealed 类」这一充分条件，v1 不碰。
- **JIT 专门去虚化**：devirt 在编译期改 IR，JIT 消费同一 IR 天然受益；不额外写 JIT 逻辑。

## Open Questions

- [ ] devirt 与 `IrInline` 的 pass 顺序：devirt 必须在 inline **之前**（先把 VCall 变 Call 才能被内联）。
      确认 `IrOptPipeline.Run` 里 inline 是首个模块级 pass → devirt 应作为**更靠前**的模块级 pass 或
      在 IrGen emit 时就地降级（本 proposal 取后者：emit 时就地发 Call，最省，天然在 inline 前）。
