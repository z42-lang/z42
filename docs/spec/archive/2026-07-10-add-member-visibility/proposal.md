# Proposal: 成员可见性元数据（unify-type-metadata P1-b）

## Why

`unify-type-metadata` 的 P1-b。现状:z42c 已算字段/方法可见性（`ExportedTypeExtractor`:
显式 `public`/`private`/`protected`,默认 `public`)并存进 **TSIG**,但**运行时 TYPE/SIGS 不带
可见性**——`TypeDesc` 字段（`FieldDesc`）只有 name/type/attrs,`FuncSig` 只有 is_static。反射
`FieldInfo`/`MethodInfo` 因此**拿不到可见性**(无 `IsPublic`)。

这正是 initiative 要补的又一样(design D1):把可见性从"只在 TSIG"补进 TYPE/SIGS。一举两得——
① 反射得到 `FieldInfo.IsPublic`/`MethodInfo.IsPublic`(C# 反射标配);② 可见性有了 TYPE/SIGS 的家
(P3 删 TSIG 的前置——EXPT 导出面 = public 可见性项派生,见 initiative D3)。TSIG 可见性 P1 并存,P3 删。

## What Changes

- **TYPE 段字段块**(实例 + 静态):每字段追加 `visibility:u8`(0=public/1=private/2=protected)。
- **SIGS 段**:每函数在 `is_static` 之后追加 `visibility:u8`。
- **z42c**:`IrFieldDesc.Visibility` + `IrFunction.Visibility`,IrGen 从 `FieldDecl.Mods`/`MethodDecl.Mods`
  填(复用 ExportedTypeExtractor 默认 public 逻辑);ZbcWriter TYPE/SIGS emit。
- **Rust**:`FieldDesc.visibility` + `FuncSig.visibility` + 读;loader 线进 `TypeDesc` 字段/方法。
- **反射**:`FieldInfo.IsPublic`/`IsPrivate` + `MethodInfo.IsPublic`/`IsPrivate`(builtin 从元数据读)。
- 版本 bump（zbc 1.22→1.23 / zpkg 0.26→0.27）+ regen + 反射测试。

> 与 enum 块不同:字段/方法可见性**非 gated**——每个字段/函数记录都长 1 字节 → 每个 zpkg 变（regen）。

## Scope

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.ir/src/IrModule.z42` | MODIFY | `IrFieldDesc.Visibility` + `IrFunction.Visibility`（默认 "public"） |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 22→23 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | BuildType 字段块 + WriteSigEntries 写 visibility(u8) |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | **读侧对称**：字段块 + SIGS 消费 visibility（非 gated 必须写读同步） |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | **读侧对称**：ReadModuleSigs 消费 SIGS visibility |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 填字段/方法 visibility（mods → 复用默认 public 逻辑） |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 26→27（SIGS/MODS 经共享 writer 自动跟随） |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `FieldDesc.visibility` + visibility→u8 常量 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | read_type 字段块 + read_sigs 读 visibility；bump `ZBC/ZPKG_VERSION_MINOR` |
| `src/runtime/src/metadata/types.rs` | MODIFY | `FieldSlot.visibility` + `Function.visibility`（bytecode.rs）携可见性 |
| `src/runtime/src/metadata/loader.rs` / `src/runtime/src/interp/dispatch.rs` / `src/runtime/src/metadata/resolver.rs` | MODIFY | 组装/base-merge/stub 线程可见性 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | build_field_info/build_method_info + resolve_func_sig 塞 IsPublic/IsPrivate slot |
| `docs/design/language/reflection.md` | MODIFY | 成员可见性反射机制节 + FieldInfo/MethodInfo 成员表 |
| `src/runtime/src/**/*_tests.rs`（loader/merge/constraint/reflection/jit/gc/exception/snapshot + tests/native_*·cross_thread） | MODIFY | FieldSlot/FieldDesc/Function 字面量补 visibility 字段 |
| `src/tests/zpkg-format/*/expected.json` | MODIFY | header minor 26→27 |
| `src/libraries/z42.core/src/Reflection/FieldInfo.z42` | MODIFY | `IsPublic`/`IsPrivate` |
| `src/libraries/z42.core/src/Reflection/MethodInfo.z42` | MODIFY | `IsPublic`/`IsPrivate` |
| `docs/design/runtime/zbc.md` / `zpkg.md` / `.claude/rules/version-bumping.md` | MODIFY | changelog + 常量表 |
| `src/tests/zbc-format/*` + `zpkg-format/*` | MODIFY | regen |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` / `z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | golden hex + header pin |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | pinned 版本常量 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | IsPublic/IsPrivate [Test] |

## Out of Scope

- 方法 virtual/abstract flags → P1-c `add-method-modifiers`。
- 从 TSIG 删可见性 → P3。
- `internal`/`file` 等更多 accessibility：MVP 只 public/private/protected（其余归 public）。

## Open Questions

- [ ] 反射 `IsPublic` 之外是否同时加 `IsPrivate`/`IsFamily`？（design 倾向先 `IsPublic`+`IsPrivate`）
