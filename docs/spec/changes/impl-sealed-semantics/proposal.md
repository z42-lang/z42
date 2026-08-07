# Proposal: sealed 修饰符语义强制 + 元数据 + 反射

> **拆分说明（2026-08-07）**：原 `impl-sealed-semantics-devirt` 含 ④ 基于 sealed 的去虚化。
> 实施中发现去虚化的**直接调用目标解析**（定义类 + RegKey + abstract 跳过 + 本地/imported 约定）
> 正确性敏感（错=静默 miscall），且需独立测试矩阵——经 User 裁决**拆为独立 follow-up
> `add-sealed-devirt`**。本 change 只落 ①②③（sealed 成为有语义的真修饰符 + 方法级 sealed 位 +
> 反射 + 跨包）。去虚化的地基（`CLASS_FLAG_SEALED` 已在、`METHOD_FLAG_SEALED` 本 change 落）
> 就绪后，follow-up 直接消费。

## Why

`sealed` 当前是**反射可见的空装饰**——能被 lexer/parser 接受、类级被序列化进 `CLASS_FLAG_SEALED`、运行时 `__type_is_sealed` 能读回，但：

- **不禁止继承**：`class B : SealedA {}` 照编不报错（`SymbolCollector` 无检查）。
- **不禁止 override**：override 一个 sealed 方法无人拦。
- **方法级 sealed 直接丢失**：`method_flags` 只有 bit0=virtual/bit1=abstract，`sealed override` 的 `sealed` 被 parser 收进 `Mods` 串后**下游不读**、不进元数据。

不做的后果：`sealed` 长期是误导性空修饰符（写了没用、反射还谎称 sealed）。补齐语义后，`sealed` 成为真修饰符，并为后续 sealed 去虚化（follow-up）铺好元数据地基。

## What Changes

- **① sealed 语义强制**：继承 sealed 类 → 编译错误（E0427）；override sealed 方法 → 编译错误（E0428）。
- **② 方法级 sealed 位**：`method_flags` 新增 `METHOD_FLAG_SEALED = 1<<2`；zbc 1.29→1.30、zpkg 34→35（联动）；反射新增 `MethodInfo.IsSealed`（VM 写入，对称 `IsVirtual`/`IsAbstract`）。
- **③ 方法上 `sealed` = `sealed override` 简写**：方法上单写 `sealed` 语义等价 `sealed override`；仍接受 `sealed override`（C# 粘贴兼容）；两种写法都强制"必须匹配基类某 virtual"，否则报错（E0429）。**纯 semantics 层**——parser 早已把 `sealed` 当合法修饰符收进 `Mods`，无需改语法。
- **跨包**：TSIG 模型（`ExportedClassZ`/`ExportedMethodZ`）携带 IsSealed（从已在 wire 的 `CLASS_FLAG_SEALED` / 本 change 落的 `METHOD_FLAG_SEALED` 提取，**无新增序列化**），`ImportedSymbolLoader` 还原到 `Z42ClassType.IsSealed` / `MethodSymbol.IsSealed`，使 ①③ 对导入类型同样生效。
- **两阶段 support/use**（因 zbc 格式 bump，遵 `bootstrap-seed.md`）：本 change 只落"支持"——z42c / stdlib / xtask **源码不写 `sealed`**，产物版本经两代自举推进；examples / tests 由当前 z42c 编译，可立即用 sealed 验证。下一 nightly 发布后才在编译器/库源码 use。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | E0427/E0428/E0429 |
| `src/compiler/z42c.semantics/src/Symbol.z42` | MODIFY | `MethodSymbol.IsSealed`（镜像 `FieldSymbol.IsReadonly`） |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42ClassType.IsSealed` |
| `src/compiler/z42c.semantics/src/IrGenFacts.z42` | MODIFY | `_methodFlags`：sealed 连带置 virtual 位 + bit2 |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 本地类/方法标 IsSealed；2 处 override 识别点认 sealed（简写参与槽对齐）；`_passSealedEnforce` + `_nearestBaseMethod` + 接入 3 条 collect 路径 |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 从 `ExportedClassZ.IsSealed` / `ExportedMethodZ.IsSealed` 还原 IsSealed（跨包强制） |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedClassZ.IsSealed` / `ExportedMethodZ.IsSealed`（post-construction 字段，构造函数元数不变 = 旧种子 ABI） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | 从 `(cd.Flags & 2)` / `(f.MethodFlags & 4)` 提取 sealed 入 TSIG 模型 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 29→30 + 注释 |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 34→35 + 注释 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `METHOD_FLAG_SEALED: u8 = 1 << 2` |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | `ZBC_VERSION_MINOR` 30 / `ZPKG_VERSION_MINOR` 35 + changelog |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `MethodInfo.IsSealed` 从 `METHOD_FLAG_SEALED`（两处构造点） |
| `src/libraries/z42.core/src/Reflection/MethodInfo.z42` | MODIFY | `IsSealed` 字段 |
| `docs/design/runtime/zbc.md` | MODIFY | Minor changelog 加 1.30 行 |
| `docs/design/runtime/zpkg.md` | MODIFY | Minor changelog 加 0.35 行 |
| `docs/book/src/language/sealed.md` | NEW | sealed 语义（禁继承/override）+ shorthand 机制页（挂 SUMMARY.md）；去虚化标 Deferred |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入 sealed.md |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引登记 sealed 强制 + 关联 change |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加 sealed 去虚化 |
| `src/compiler/z42c.semantics/tests/sealed/inherit_sealed_class_error/source.z42` | NEW | ① 继承 sealed 类报错 |
| `src/compiler/z42c.semantics/tests/sealed/override_sealed_method_error/source.z42` | NEW | ① override sealed 方法报错 |
| `src/compiler/z42c.semantics/tests/sealed/sealed_shorthand_ok/source.z42` | NEW | ③ 方法单写 `sealed` == `sealed override` |
| `src/compiler/z42c.semantics/tests/sealed/sealed_non_override_error/source.z42` | NEW | ③ `sealed` 无匹配基类 virtual → 报错 |
| `src/tests/reflection/method_is_sealed/source.z42` | NEW | ② `MethodInfo.IsSealed` 反射（配 expected_output.txt） |
| `src/tests/reflection/method_is_sealed/expected_output.txt` | NEW | ② 期望输出 |
| `src/tests/zbc-format/*/source.zbc`（6 基线） | MODIFY | 格式 bump fixture 重生（`xtask build test`） |
| `src/tests/zpkg-format/*/source.zpkg`（4 基线） | MODIFY | zpkg fixture 手工重生 |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | golden hex（header minor 变化）重截 |
| `examples/sealed.z42` | NEW | 使用 sealed / sealed override 示例（当前 z42c 编） |

**只读引用**：`IrGen.z42`（method_flags 装配）、`ClassDescBuilder.z42`（`CLASS_FLAG_SEALED` 序列化，已存在不改）、`.claude/rules/version-bumping.md` / `bootstrap-seed.md`。

## Out of Scope

- **④ sealed 去虚化**（VCall→Call 解锁内联）——拆为 follow-up `add-sealed-devirt`。原因：直接调用目标解析（定义类 + RegKey + abstract 跳过 + 本地/imported 约定）正确性敏感，需独立测试矩阵。地基（class/method sealed 位 + 跨包 IsSealed）本 change 落齐，follow-up 直接消费。
- **JIT / AOT**——interp 优先纪律。
- **在 z42c / stdlib / xtask 源码使用 sealed**——两-nightly 纪律，属下一 change。

## Open Questions

- [ ] 无（去虚化拆出后，本 change 的开放问题清零）。
