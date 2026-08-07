# Design: 基于 sealed 的去虚化

## Architecture

```
 a.M()  where a : sealed class A            ExprEmitter._emitCall (instance 分支)
   │                                          │
   ▼                                          ▼
 c.Receiver.Type() = Z42ClassType A     ┌── SealedReceiverClass(recvType)? ──┐
 A.IsSealed = true                      │  非 sealed 类 / 接口 / cast-Unknown │→ 原样 VCallInstr
                                        │  → 否                               │
                                        └── 是 ──▼
                                    ResolveSealedTarget(A, "M", argc):
                                      沿 A→base 链找**最近声明 M 且非 abstract**的类 C
                                      target = FQ(C) + "." + RegKey(C.M)
                                      （FQ/RegKey 逐字节匹配 IrGen: _q(_classIrShortName(C))+"."+md.RegKey）
                                        │
                                    target == "" (越 v1 边界/不可解析) ──→ 原样 VCallInstr
                                        │ target 非空
                                        ▼
                                    CallInstr(dst, target, [recv, ...args], argc+1)
                                        │
                                        ▼
                                    IrInline 可内联该直接 Call（VCall 吃不进）
```

## Decisions

### Decision 1: 落点——IrGen emit 时就地降级，而非独立 opt pass

**问题：** devirt 放独立 `IrOptPipeline` pass，还是 `_emitCall` emit 时就地？

**决定：** **emit 时就地**（`ExprEmitter._emitCall`：sealed receiver → 直接发 `CallInstr` 而非 `VCallInstr`）。
**理由：**
- 天然在 `IrInline` **之前**（IR 一产出就是 Call）→ 无需协调 pass 顺序（proposal Open Question 消解）。
- emit 处已有全部上下文（`c.Receiver.Type()` 静态类型、`c.OwnerClass`、既有守卫）——复用 `_emitCall`
  现有的 owns/ifaceRecv/cast-Unknown 判定，devirt 作为其中一条新分支，改动最小。
- 门控：`Opt.Devirt` 位（`--no-opt devirt` 关）→ 关时走原 VCall，供 before/after 对拍（主正确性门）。

### Decision 2: v1 安全边界——本地非泛型 sealed 类 + 本地定义类 + 非 abstract 目标

**问题：** 去虚化的充分条件与 v1 覆盖面。

**充分条件（可靠）：** receiver 静态类型是 sealed 类 `A` → `A` 不可被继承 → runtime 类型必是 `A`
→ `A.M` 的 vtable 目标唯一、编译期可解析。这对 imported/泛型 sealed 同样成立**语义上**，但**目标名
的精确构造**在这些情形更易错。

**v1 只覆盖：**
- receiver 静态类型是 **本地**（`LocalClasses`）**非泛型**（`TypeParams.Count == 0`）sealed 类；
- 目标定义类 `C` 也是本地非泛型（沿基链，遇 imported/泛型基类即停、返回 "" → 回落 VCall）；
- 目标方法**非 abstract**（abstract 无 body、不能作直接调用目标）。

**越界即返回 ""（回落 VCall，永远安全）：** imported receiver、泛型 sealed、目标定义类为 imported/泛型、
目标 abstract、`sealed override` 但 receiver 是基类型（此时 receiver 静态类型非 sealed 类 →
`SealedReceiverClass` 本就为否）。

**理由：** 「返回 "" 就回落 VCall」使**任何解析不确定的情形都退回安全的虚派发**——去虚化是纯优化，
错过 = 不优化（可接受），绝不 miscall。v1 把「目标名精确构造」限制在最可控的本地非泛型域。

### Decision 3: 目标名必须逐字节匹配 IrGen 的函数命名（正确性铁律）

**问题：** 直接 `CallInstr` 的目标字符串错一个字符 = 运行期 `undefined function` 或**静默调错**。

**决定：** `ResolveSealedTarget` 产出的目标名 = `IrGen` 发射该函数时用的**同一构造**：
`_q(_classIrShortName(C)) + "." + md.RegKey`。具体：
- `FQ(C)`：本地类用当前模块 ns 前缀（`EmitContext.Qualify` ≡ `IrGen._q`，同包同 ns 场景已验等价）；
  **跨-ns 本地类**用 IrGen 的 `_classFqName` 同款逻辑（v1 若定义类跨 ns 且不确定 → 保守返回 ""）。
- `RegKey`：定义类 `C` 的 `MethodSymbol.RegKey`（bare / `Name$arity` / `Name$arity$types`）——
  从 `ct(C).Methods.Get(<解析键>)` 取，**不自己拼** arity/types。
- 非泛型限制正是为规避 `_classIrShortName` 的 `$N` mangle 分支。

**验证兜底：** Decision 4 的 before/after 对拍是这条铁律的**运行期检查**——若目标名错，去虚化版输出
必与 VCall 版不同 → 对拍红。

### Decision 4: 主正确性门 = `--no-opt devirt` before/after 逐字节对拍

devirt 是纯优化：**开/关 devirt，可观察输出必须逐字节相同**。每个 e2e 用例跑两遍
（`Opt.Devirt` 开 / 关）对比 stdout。这是「目标解析对不对」的**端到端铁证**——比只看 emit 的
`call @X` 字符串更强（字符串对但目标不存在/错，对拍会抓到）。

## Implementation Notes

- `SealedReceiverClass(recvType)`：`recvType as Z42ClassType` → 非 null 且 `IsSealed` 且本地
  （`LocalClasses.ContainsKey(ct.Name())`）且非泛型（`GenericParamCount == 0`）→ 返回 ct，否则 null。
- `ResolveSealedTarget(ct, method, argc)`：沿 ct→base 链（`Symbols.GetClass(BaseName)`）找最近
  `ct2.Methods` 命中 method（按 name + arity/签名，复用 `_nearestBaseMethod` 同款匹配）的 `MethodSymbol ms`，
  要求：ms 有 body（非 abstract：查 `ms.Decl.Mods` 无 "abstract" / 或 OwnMethodIsAbstract）、ms 所在类
  ct2 本地非泛型。命中 → `QualifyClass(ct2.Name()) + "." + ms.RegKey`；任一不满足 → ""。
- `_emitCall` 注入点：line 730 instance 分支内、line 763-766 VCall fallback **之前**：
  ```
  if (Opt.Has(optSet, Opt.Devirt)) {
    Z42ClassType sc = this._ctx.SealedReceiverClass(c.Receiver.Type());
    if (sc != null && !this._castUnknownChain(c.Receiver)) {
      string tgt = this._ctx.ResolveSealedTarget(sc, c.MethodName, c.ArgCount);
      if (tgt != "") { emit CallInstr(dst, tgt, [recv, ...iargs], argc+1); return dst; }
    }
  }
  // …既有 owns/DepIndex/VCall fallback 不变
  ```
- **cast-Unknown 守卫**：cast-to-class 的 receiver 静态类型可能被标 Unknown/仍是 sealed 类——
  devirt 前显式跳过 `_castUnknownChain`（与 itag=Unknown 同源），避免对 cast receiver 去虚化。
- devirt 改 IR 输出 → 当次 gen1≠gen2 破一代自举（D7）→ warm 重建自愈（与优化管线历次同款）。

## Testing Strategy

- **e2e before/after 对拍（主门）**：3 用例（本地 sealed 自身声明 / 继承基类未 override / 非 sealed 对照）
  各跑 `Opt.Devirt` 开+关，stdout 逐字节一致。
- **codegen 单测**：sealed receiver → dump IR 见 `call @Ns.Cls.M`（非 `vcall`）；非 sealed → `vcall`；
  继承未 override → `call @Ns.Base.M`（目标是基类实现）。
- **内联验证**：sealed receiver 调用被 `IrInline` 内联（dump 见调用点展开）。
- **回落安全**：泛型 sealed / imported sealed receiver → 仍 `vcall`（v1 不去虚化，对拍仍一致）。
- **GREEN**：完整 `xtask test`（含 z42c 自举不动点）。
