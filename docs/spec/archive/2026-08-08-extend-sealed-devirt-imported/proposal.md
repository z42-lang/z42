# Proposal: 去虚化扩到 imported sealed 类

## Why

`add-sealed-devirt`（#142）的去虚化 v1 只覆盖**本地**非泛型 sealed 类 receiver——`SealedReceiverClass` /
`ResolveSealedTarget` 里的 `LocalClasses.ContainsKey` 守卫对 imported 类返回空 → 回落 VCall。

但 imported sealed 类（尤其 stdlib 里 `sealed class` 被用户/z42c 代码按其精确类型调用）同样满足
「sealed 不可继承 → 静态类型即 runtime 类型 → 目标编译期唯一」——只是 v1 保守未做，因跨包目标名构造
更易错。本 change 把去虚化扩到 imported sealed，解锁这些调用点的内联。

**探索确认的地基**（已就绪，无需新增）：
- imported 类经 `SymbolCollector._mergeImports` 进 `Symbols`（`ResolveSealedTarget` 沿基链已能 walk）。
- imported `Z42ClassType.IsSealed` 由 `ImportedSymbolLoader` 从 `CLASS_FLAG_SEALED` 还原（#140）。
- imported `MethodSymbol.RegKey` 由 `ImportedSymbolLoader` 从 TSIG 设 = **导出包发射该函数用的 key**。
- `EmitContext.QualifyClass` 对 imported 类经 `ImportedClassNs` 返回**其源 ns**前缀 → `QualifyClass(imp)+"."+RegKey`
  = 导出包发射的函数名（与本地 `_q(ns)+RegKey` 同构）。

## What Changes

**放宽 `SealedReceiverClass` / `ResolveSealedTarget` 的「仅本地」守卫，接纳 imported sealed 类**——
把「in `LocalClasses`」改为「in `LocalClasses`（本地，QualifyClass=当前 ns）**或** in `ImportedClassNs`
（imported，QualifyClass=源 ns）」。两者 `QualifyClass(cls)+"."+RegKey` 都精确等于该函数的发射名；
**都不满足 → 返回 ""（回落 VCall，永不 miscall）**。

- 非泛型 + 非 abstract + 「不确定即回落」三条 v1 铁律**不变**。
- 目标名构造**不变**（`QualifyClass(定义类)+"."+RegKey`）——只是 `QualifyClass` 对 imported 走 `ImportedClassNs` 分支。
- 沿基链 walk 遇「既不在 LocalClasses 也不在 ImportedClassNs」的类（无法确定 ns）→ 返回 ""（保守）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY | `SealedReceiverClass`：`LocalClasses` 守卫 → `_devirtQualifiable(name)`（in LocalClasses 或 ImportedClassNs）。`ResolveSealedTarget`：同守卫 + imported 定义类经 `_depHasFunction` 校验 FQ 真实发射（排除 TSIG 展平的继承方法，见 design Decision 2.5）。新增 `_devirtQualifiable` + `_depHasFunction` 两私有助手 |
| `src/tests/cross-zpkg/sealed_devirt_imported/...` | NEW | 跨包 e2e：pkgA 导出 sealed 类（含 virtual + 继承），pkgB 调其精确类型 → 去虚化 + 结果正确（配 expected） |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | 单测：imported sealed receiver → `call @<srcNs>.Cls.M`；非泛型/非 abstract 边界；`--no-opt devirt` 对拍 |
| `docs/book/src/language/sealed.md` | MODIFY | 去虚化节：v1 边界从「仅本地」更新为「本地 + imported」；Deferred 段移除 imported 项（保留泛型/sealed-override） |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index：imported 项落地，保留泛型/sealed-override |
| `src/compiler/z42c.semantics/README.md` | MODIFY | ExprEmitter 行补 imported |

**只读引用**：`ImportedSymbolLoader.z42`（imported IsSealed/RegKey 来源）、`DependencyIndex.z42`（探索发现
GetInstance 是 bare-name 键 + 歧义排除 → 低覆盖，故**不走 GetInstance**、走 Symbols+QualifyClass）、
`SymbolCollector._mergeImports`（imported 类入 Symbols）。

## Out of Scope（保留 Deferred）

- **泛型 sealed 类**（`$N` mangle + 类型参数替换）——非泛型铁律不变。
- **`sealed override`**（receiver 是基类型）——非「receiver 静态即 sealed 类」充分条件。
- **跨包目标名若与发射名不匹配的边角**（如 imported 类未进 ImportedClassNs）→ 返回 ""，不强行构造。

## Open Questions（已在实现中解决）

- [x] 验证 `QualifyClass(imported)+"."+RegKey` 与导出包发射名**逐字节一致**的边角：cross-zpkg e2e **抓到真 bug**
      ——imported 类 `Methods` 因 TSIG 展平**含继承方法**，`Leaf.Methods.ContainsKey("Tag")` 命中却构造出从未发射的
      `Demo.Sld.Leaf.Tag` → 运行期 `undefined function`。**解决**：imported 定义类候选返回前用 `Deps.Statics.ContainsKey(FQ)`
      校验真实发射，未命中即沿基链继续上溯到真声明类（design Decision 2.5）。本地路径先行短路、零改动。
