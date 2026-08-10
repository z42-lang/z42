# Proposal: 去虚化扩到 sealed override + 泛型 sealed

## Why

`add-sealed-devirt`（#142）+ `extend-sealed-devirt-imported`（#147）的去虚化只在
**receiver 静态类型是非泛型 sealed 类**（本地或 imported）时触发。Deferred backlog 里 sealed 线还剩两项，
本 change 一并落地：

1. **sealed override**（①）——非 sealed 类上的 `sealed override` 方法。整类未 sealed，但该**方法**不可再被
   override → 从「能看见这个 sealed 方法」的静态类型 `S` 起，运行期类型 `R ≤ S` 的最派生实现唯一 = 声明该
   sealed override 的类。这是比「整类 sealed」常见得多的模式（stdlib / 用户代码大量用 `sealed override`
   钉死某个热点虚方法而不封整类）。
2. **泛型 sealed**（②）——`sealed class Box<T>`。#142 起用非泛型铁律（`GenericParamCount>0 → 回落`）规避
   `$N` arity-mangle；本项接纳泛型 sealed receiver（含 imported），去虚化目标名走 arity-mangle 短名
   `Name$N`，逐字节匹配 `IrGen._classIrShortName`。

两项都满足去虚化的根本前提「目标编译期唯一」，只是 v1/v2 保守未做。

**探索确认的地基**（已就绪，无需新增）：
- `MethodSymbol.IsSealed`（#140 `impl-sealed-semantics-devirt`）——本地由 `SymbolCollector`（`_hasWord(md.Mods,"sealed")`）、
  跨包由 `ImportedSymbolLoader`（TSIG method_flags bit2）填充。① 的方法级判据直接读它。
- `Z42InstantiatedType.Def`（泛型实例的定义类，携 `IsSealed` / `GenericParamCount`）+ `_classIrShortName`
  的 `Name$N` mangle 规则（`IrGen.z42:105`）——② 的目标名构造依据。
- `_devirtQualifiable` / `_depHasFunction` / `ResolveSealedTarget` 基链 walk（#142/#147）——两项均复用。

## What Changes

### ① sealed override
- `SealedReceiverClass` → 泛化为 **`DevirtReceiverClass`**：不再要求 `ct.IsSealed`，接纳任意可限定非泛型类
  （sealed 与否都返回）。是否 sealed 交由调用方作 `classSealed` 传入。
- `ResolveSealedTarget` 加 `bool classSealed` 参数。基链 walk 到 **declClass**（最近声明该 key 的可限定类）后，
  门控 **`classSealed || ms.IsSealed`**：
  - `classSealed`（整类 sealed）：`R==startCt`，目标必唯一 → 命中（原 #142 行为，逐字节不变）。
  - `ms.IsSealed`（sealed override）：declClass 下无人能 override → `R ≤ startCt ≤ declClass` ⟹ R 的最派生
    实现 = declClass 的 → 命中。
  - 皆非：R 可在 declClass 下再 override → 目标不唯一 → `""`（回落 VCall，永不 miscall）。
- 调用点 `ResolveSealedTarget(sc, key, argc, sc.IsSealed)`。

### ② 泛型 sealed
- `DevirtReceiverClass` 接纳 `Z42InstantiatedType`：解包 `.Def`，去掉 `GenericParamCount>0 → null` 铁律。
- 目标名构造从 `QualifyClass(Name)` 换为 **arity-mangle 短名**（`Name$N`）——逐字节匹配 IrGen 发射。
- imported 泛型 sealed 复用 `_depHasFunction`（FQ 用 `$N` 名）排除 TSIG 展平的继承方法。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY | ① `SealedReceiverClass`→`DevirtReceiverClass`（去 IsSealed 铁律）+ `ResolveSealedTarget` 加 `classSealed` 参数与方法级 sealed 门控；② 接纳 `Z42InstantiatedType`.Def + `$N` mangle 目标名 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | 调用点：`DevirtReceiverClass` + 传 `sc.IsSealed` |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | ① `test_sealed_override_devirt` / `test_nonsealed_override_stays_vcall`；② 泛型 sealed 单测 + 非 sealed 泛型对照 |
| `src/tests/classes/sealed_override_devirt.z42` | NEW | ① e2e：sealed override 去虚化结果 == 虚派发 + 子类继承 sealed 方法正确 + 非 sealed override 保持多态 |
| `src/tests/classes/sealed_generic_devirt.z42`（或 cross-zpkg） | NEW | ② e2e：泛型 sealed receiver 去虚化正确 |
| `docs/book/src/language/sealed.md` | MODIFY | 去虚化节：边界更新为「本地/imported + sealed override + 泛型」；Deferred 段清空 sealed 线两项 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog：sealed override / 泛型 sealed 落地 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | ExprEmitter 行补 sealed override + 泛型 |

**只读引用**：`Symbol.z42`（`MethodSymbol.IsSealed`）、`Z42Type.z42`（`Z42InstantiatedType.Def`）、
`IrGen.z42`（`_classIrShortName` 的 `$N` mangle 规则）、`ImportedSymbolLoader.z42`（跨包 IsSealed / TSIG 展平）。

## Out of Scope

- **非虚方法 / 接口 receiver / cast-unknown 链**——既有守卫优先，不动。
- **跨过程 / 数据流型别精化**（如 `if (x is Dog)` 后窄化）——仍按静态声明类型。

## Open Questions（实现中解决）

- [x] ① sealed override 正确性判据：`R ≤ startCt ≤ declClass` + declClass 处 sealed ⟹ 目标唯一（design Decision 1）。
- [ ] ② `QualifyClass(Name$N)` 与 IrGen 发射名逐字节一致的边角（尤其 imported 泛型 sealed）——以自举不动点 + e2e 为门。
