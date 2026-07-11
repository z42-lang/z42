# Proposal: delegate 元数据（unify-type-metadata P1-e ②）

## Why

delegate 目前是**纯编译期函数型别名**（`SymbolTable.Delegates` → Z42FuncType），运行期实例为
FuncRef/Closure——**TYPE 无条目**：`Type.GetType("<fq delegate>")` 拿不到句柄、无法反射签名。
且 **TSIG 携带 delegate 定义**（含泛型 Action/Func/Predicate 内建 11 个）供跨包解析——P3 删
TSIG 前 delegate 必须能从 TYPE/SIGS 重建（initiative D5：delegate-as-class，硬前置）。

## What Changes

- **TYPE**：每个 `DelegateDecl`（含泛型）emit 一条 IrClassDesc——FQ 名 + `class_flags`
  **bit6=delegate**（0x40）+ TypeParams（泛型 delegate 的 tps 存 TYPE 条目，Invoke 签名按名引用）。
  无字段/无 enum 块（bit6 无额外 payload——但沿 1.19 interface 先例，flags 语义扩展 + 新增条目
  → minor bump）。
- **SIGS/FUNC**：合成 `<FQ>.Invoke` **死体桩**（复用 P1-c abstract-stub 机制：实例、`ret null`/
  `ret`，永不被调——真实调用走 CallIndirect）；签名 = delegate 声明（参数源拼写类型 + 参数名 +
  P1-d 全套参数元数据）；`method_flags` bit0=virtual（镜像 C# delegate Invoke）。
- **反射**：`Type.IsDelegate`（`__type_is_delegate` builtin，读 bit6）；`GetMethods()` 自动含
  Invoke（own_methods 前缀扫描）→ 签名经既有 MethodInfo/ParameterInfo 面反射。
- **typeof(MyDelegate)**：不动 `ResolveTypeP`（delegate 名必须继续解析为函数类型，改了会破坏
  delegate 赋值检查）→ typeof 对 delegate 名维持现状；反射入口用 `Type.GetType(fq)`。
  typeof(delegate) → Deferred。
- **格式 bump**：zbc 1.25→1.26 / zpkg 0.29→0.30（两代自举）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | DelegateDecl → IrClassDesc（bit6+tps）+ `_emitDelegateInvoke` 合成桩 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | Minor 25→26 |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | Minor 29→30 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `CLASS_FLAG_DELEGATE = 1<<6` |
| `src/runtime/src/metadata/types.rs` | MODIFY | `TypeDesc.is_delegate()`（class_flags 派生） |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | bump 常量 26/30（TYPE 记录布局不变，仅语义位） |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `builtin_type_is_delegate` |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 `__type_is_delegate` |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | `IsDelegate` extern getter |
| `docs/design/language/reflection.md` | MODIFY | delegate 反射节 + Type 成员表 + typeof(delegate) Deferred |
| `docs/design/runtime/zbc.md` / `zpkg.md` / `.claude/rules/version-bumping.md` | MODIFY | changelog + 常量表 |
| `src/tests/zbc-format/*` + `zpkg-format/*` | MODIFY | regen |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` / `z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | golden hex + header pin |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | pinned 26/30 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | IsDelegate + Invoke 签名 [Test] |

**只读引用**：`docs/spec/archive/2026-07-09-add-enum-type-metadata/`（enum-as-TYPE 同款范式）、
`src/compiler/z42c.semantics/src/FunctionEmitter.z42`（_sigTypeName 源拼写口径）。

## Out of Scope
- typeof(MyDelegate) 直达句柄（需 typeof 路径特判 Z42FuncType，Deferred）
- MulticastDelegate 基类层次 / DynamicInvoke（0.4.x+）
- P2 重建路径本身（下一 change）

## Open Questions
（无——D5 已定 delegate-as-class；tps 放 TYPE 条目由 P2 重建口径决定）
