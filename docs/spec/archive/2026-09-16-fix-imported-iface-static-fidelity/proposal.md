# Proposal: 接口方法 static 保真度（跨包 static-abstract 成员满足性）

## Why

跨包（导入）接口的 static-abstract 成员，其 `IsStatic` 从导出/wire 一路丢成 `false`——
**根因是接口方法在 zbc wire 里根本不携带任何修饰符位**（`name/ret/pcount/ptypes`，零 flags）。
链路上每一跳都因此失真：

- `ClassDescBuilder`（第 0 层，死因）建接口方法块时从不读 `static`；
- zbc 接口方法块（TYPE 段，`Flags bit4` gated）无 flags 字节、无预留位；
- `TsigReconcile._rebuildInterface` 只能硬编码 `isStatic=false`；
- `ImportedSymbolLoader:391` 虽已读 `mz.IsStatic`，但读到的恒 false（上游根本没写）。

后果：#636（`fix-iface-satisfaction-gaps`）被迫在 `InheritanceResolver._checkOneIfaceMethod`
加一处**临时守卫** `if (it.IsImported) { return; }`——跳过导入接口的 static/可见性/返回类型
校验。不跳则 `struct Money : INumber`（INumber 导入自 z42.core）的 5 个 `static override`
被误判「接口声明为 instance」→ 5×E0412 假红。

这是 stopgap，不是根因修复。本 change 从**产出端**（wire 格式）修：让接口方法像普通方法
（SIGS 早有 `is_static:u8`）一样携带真实 static 位，然后删掉守卫，使导入接口也得到完整的
满足性校验（对齐本包接口）。

## What Changes

- **zbc/zpkg 格式 bump**（zbc 1.40→1.41、zpkg 0.45→0.46）：接口方法块每方法追加一个
  `is_static:u8`（镜像 SIGS 第 445 行的 `is_static` 专用字节），位置在 `pcount` 之后、`ptypes` 之前。
- `IrClassDesc` 加平行数组 `IfaceMethodStatic:int[]`（0/1），`ClassDescBuilder` 从
  `_hasWord(imd.Mods, "static")` 填充。
- z42c writer/reader（`ZbcWriter`/`ZbcReader`）对称写读该字节。
- `TsigReconcile._rebuildInterface` 用真值构造 `ExportedMethodZ`：`isStatic` = 真值，
  `isVirtual` = `!isStatic`（静态接口成员非虚），`isAbstract` = true（接口方法恒抽象）。
- runtime `type_reader.rs` 读该字节，`IfaceMethodSig` 加 `is_static: bool`（顺带为
  `Type.GetMethods()` 反射铺路；VM 派发仍走 vtable，行为不变）。
- 删 `InheritanceResolver._checkOneIfaceMethod` 的 `if (it.IsImported) { return; }` 守卫
  + 更新其上下文注释。导入接口自此得到完整 static/可见性/返回校验。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | `IrClassDesc` 加 `IfaceMethodStatic` 平行数组 + ctor 初始化 |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 建接口方法块时填充 static 平行数组 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 接口方法块写 `is_static:u8` |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | 接口方法块读 `is_static:u8` |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `_rebuildInterface` 用真 static 值构造 ExportedMethodZ |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 40→41 + 注释 |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 45→46 + 注释 |
| `src/runtime/src/metadata/zbc_reader/type_reader.rs` | MODIFY | 接口方法块读 `is_static` 字节 |
| `src/runtime/src/metadata/bytecode/class.rs` | MODIFY | `IfaceMethodSig` 加 `is_static: bool` |
| `src/runtime/src/metadata/zbc_reader/versions.rs` | MODIFY | `ZBC_VERSION_MINOR` 41 + `ZPKG_VERSION_MINOR` 46 + changelog |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | 版本断言 40→41 / 45→46 |
| `src/compiler/z42c.semantics/src/InheritanceResolver.z42` | MODIFY | 删 `it.IsImported` 守卫 + 更新注释 |
| `src/tests/zbc-format/*/source.zbc` | MODIFY | 6 个 committed zbc 字节基线重生 |
| `src/tests/zpkg-format/*/source.zpkg` | MODIFY | 4 个 committed zpkg 字节基线重生 |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | golden hex（empty header minor 随 bump 变） |
| `src/compiler/z42c.semantics/tests/inheritance/*` | NEW | 跨包 static-abstract 满足性回归测试（若适用） |
| `src/tests/cross-zpkg/iface_static_cross_pkg/` | NEW | 跨包 static-abstract 接口满足性 e2e |
| `docs/design/runtime/zbc.md` | MODIFY | Minor changelog 加 1.41 行 |
| `docs/design/runtime/zpkg.md` | MODIFY | Minor changelog 加 0.46 行 |
| `docs/book/src/language/generics.md` | MODIFY | 接口满足性覆盖导入接口 static/返回校验 |

**只读引用**：
- `src/libraries/z42.ir/src/ExportedTypes.z42` — `ExportedMethodZ` ctor 签名（isStatic/isVirtual/isAbstract）
- `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` — 导入侧已读 `mz.IsStatic`（step 7 已就绪，不改）
- `src/tests/operators/static_abstract_operator.z42` — 现有 `struct Money : INumber` 受益点

## Out of Scope

- **B1 之外的 wire flags**（virtual/abstract 独立字节、sealed 等）：接口方法的 virtual/abstract
  由 static 位派生即可（regular=virtual+abstract、static=abstract-only），不额外占字节。
- **`Type.GetMethods()` 反射暴露 static-ness**：`IfaceMethodSig.is_static` 已读入但不接反射面
  （独立议题）。
- 跨包关联类型（B3）/ 通用擦除收紧（B2）/ 带实参约束匹配（B4）——各自独立 change。

## Open Questions

无（设计已锁：镜像 SIGS `is_static:u8` 先例）。
