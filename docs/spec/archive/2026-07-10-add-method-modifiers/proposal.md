# Proposal: 方法修饰符元数据（unify-type-metadata P1-c）

## Why

`MethodInfo.IsVirtual` 当前由**运行期启发式**给出——`build_method_info` 按方法是否出现在
vtable（`is_virtual` 参数）判定，而非方法的**声明修饰符**。这不精确（vtable 成员未必 1:1
对应 `virtual`/`override`/`abstract` 声明），且**无法表达 `abstract`**（反射无 `IsAbstract`）。

unify-type-metadata 的目标是「运行期反射元数据 = 单一真相」。P1-c 把方法的
`virtual`/`abstract` 声明修饰符持久化进 SIGS，让 `MethodInfo.IsVirtual` 变为**权威**（源自
声明而非 vtable 猜测）并新增 `IsAbstract`。这是继 P1-a（enum）、P1-b（可见性）之后的第三砖。

## What Changes

- SIGS 每函数在 `visibility` 之后追加 **`method_flags:u8`**（bit0=virtual / bit1=abstract；
  static 仍由既有 `is_static` 字节表达，不重复）。**非 gated**（每函数固定 +1 字节）。
- z42c：`IrFunction.MethodFlags`（IrGen 从 `MethodDecl.Mods` 计算：`virtual`/`override`/
  `abstract` → virtual 位；`abstract` → abstract 位）；ZbcWriter 写、双 reader（ZbcReader +
  ZpkgReader）读。
- Rust：`FuncSig.method_flags` + `Function.method_flags`（reader 灌入，与 `is_static`/
  `visibility` 同源同路径）；`build_method_info` 从 `Function.method_flags` 设
  `IsVirtual`/`IsAbstract`（取代 vtable-presence 启发式）。
- 反射：`MethodInfo.IsAbstract`（新增）；`IsVirtual` 改为源自 flag。
- **abstract 方法 signature-only emission（scope 扩展，User 裁决）**：实例 `abstract` 方法此前无
  body 完全不 emit → 反射看不到 → `IsAbstract` 观测不到 true。IrGen 新增 `_emitAbstractStub`：为
  实例 abstract 方法发一个死体桩（`ret null`/`ret`，override 经 vtable 派发故永不被调）进 SIGS/FUNC
  → `Function.method_flags` 带 abstract 位 → 反射可见。**限实例 abstract**（`static abstract` 接口
  成员如 `INumber` 不动，另属静态抽象接口特性）；纯 codegen、无格式二次 bump、z42c/stdlib 源无实例
  abstract 方法故零 byte 漂移（自举不动点不受影响）。
- 格式 bump：zbc 1.23→1.24 / zpkg 0.27→0.28（两代自举，同 P1-b）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.ir/src/IrModule.z42` | MODIFY | `IrFunction.MethodFlags:int`（默认 0） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | `_methodFlags(mods)` 助手；显式方法/impl 方法/override 处填 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 23→24 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | WriteSigEntries visibility 后写 method_flags(u8) |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | SIGS visibility 后读 method_flags（读侧对称） |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | ReadModuleSigs visibility 后读 method_flags（读侧对称） |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 27→28 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `FuncSig.method_flags` + `Function.method_flags` + METHOD_FLAG_* 常量 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | read_sigs 读 method_flags；Function 灌入；bump `ZBC/ZPKG_VERSION_MINOR` |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | build_method_info + resolve_func_sig 塞 IsVirtual/IsAbstract（源自 flag） |
| `src/libraries/z42.core/src/Reflection/MethodInfo.z42` | MODIFY | `IsAbstract` 新增（`IsVirtual` 已存在） |
| `docs/design/language/reflection.md` | MODIFY | 方法修饰符反射机制节 + MethodInfo 成员表 |
| `docs/design/runtime/zbc.md` / `zpkg.md` / `.claude/rules/version-bumping.md` | MODIFY | changelog + 常量表 |
| `src/tests/zbc-format/*` + `zpkg-format/*` | MODIFY | regen（fixtures + expected.json minor） |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` / `z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | golden hex + header pin |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | pinned 版本常量 24/28 |
| `src/runtime/src/**/*_tests.rs`（FuncSig/Function 字面量） | MODIFY | 补 method_flags 字段 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | IsVirtual/IsAbstract [Test] |

**只读引用**：
- `src/compiler/z42c.syntax/src/Decl.z42` — 确认 `MethodDecl.Mods` 承载 virtual/abstract/override
- `docs/spec/archive/2026-07-10-add-member-visibility/` — P1-b 同款实现范式参考

## Out of Scope

- minArg / 默认值 / varargs / 参数名（P1-d `add-param-metadata`）
- delegate 元数据 + 跨包 impl 反射（P1-e）
- 删 TSIG（P3）

## Open Questions

- [ ] `IsVirtual` 语义：virtual/override/abstract 三者都置 bit0（镜像 C# `IsVirtual`）？（倾向是）
