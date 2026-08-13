# Proposal: 补全类级访问控制（反射面 + 不一致可访问性 + 顶层声明拒绝 + 接口可见性）

## Why

跨包 internal 类引用强制（#183/#184）落地后，[access-control 规范](../../../design/language/access-control.md)的**类级访问**主干已成，但留下四块 Deferred 后续，本 change 一次补齐（四项均**无格式 bump**——#184 的 TYPE 可见性字节已在线且对每条 TYPE record 无条件写/读，接口 TYPE 亦已携带）：

1. **类可见性反射面**：可见性字节现被 VM `read-and-discard`，无反射面。补 `Type.IsPublic` 等（对齐 C# `System.Type` 可见性谓词）。
2. **不一致可访问性**（C# CS0050–53/60/61）：`public` 签名/基类暴露更低可见性的类型 → 当前无诊断，静默泄漏封装。
3. **顶层类/接口/枚举/函数标 `private`/`protected`**：模块作用域下这两级无意义，当前静默接受（按 internal 处理），应声明期拒绝。
4. **接口类型可见性**：`Z42InterfaceType` 未建模可见性 → `internal` 接口可被跨包引用，`CheckTypeRef` 在 `is Z42ClassType` 处放行接口。补齐建模 + 强制，与类对称。

## What Changes

- **① 反射**：VM 存可见性字节入 `ClassDesc`/`TypeDesc`；新 6 个 builtin 谓词；`Type.z42` 加 6 个 extern 属性（`IsPublic`/`IsNotPublic`/`IsNestedPublic`/`IsNestedPrivate`/`IsNestedFamily`/`IsNestedAssembly`，C# 命名）。
- **② 不一致可访问性**：新诊断 `E0441`；`AccessChecker` 加 `CheckExposure`，在 `DeclBinder._bindClass`（TypeChecker bag，可测）比对成员/基类可见性 vs 被暴露类型可见性。
- **③ 顶层拒绝**：新诊断 `E0442`；`Parser.ParseCompilationUnit` 分派点对顶层 class/interface/struct/record/enum/函数拒绝 `private`/`protected`（parser bag，可测）。
- **④ 接口可见性**：`Z42InterfaceType.Visibility` 字段；采集/降级/导入/reconcile 各接口分支补齐；`CheckTypeRef` 加接口分支。复用 #184 已在线的 TYPE 可见性字节，**无 bump**。

## Scope（允许改动的文件）

### ① 类可见性反射（runtime + stdlib）
| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `ClassDesc` 加 `visibility: u8` |
| `src/runtime/src/metadata/types.rs` | MODIFY | `TypeDesc` 加 `visibility: u8` + 便捷访问 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 可见性字节从 read-and-discard 改为存入 `ClassDesc.visibility` |
| `src/runtime/src/metadata/loader.rs` | MODIFY | `ClassDesc.visibility` → `TypeDesc.visibility` 线程 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | 6 个 `builtin_type_is_*` 可见性谓词 |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 6 个 builtin |
| `src/libraries/z42.core/src/Type.z42` | MODIFY | 6 个 `[Native]` extern bool 属性 |
| `src/runtime/src/corelib/reflection_tests.rs` | MODIFY | Rust 单测（可见性字节 decode + 谓词） |
| `src/tests/types/type_visibility.z42` | NEW | golden：顶层/嵌套 × 四级可见性反射断言 |

### ② 不一致可访问性（compiler）
| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 加 `InconsistentAccessibility = "E0441"` |
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | MODIFY | 加 `CheckExposure`（静态）+ 可见性 rank helper |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | `_bindClass` 调 `CheckExposure` 遍历成员/基类 |
| `src/compiler/z42c.semantics/tests/access-control/access_control_tests.z42` | MODIFY | E0441 正/反用例 |

### ③ 顶层 private/protected 拒绝（compiler）
| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 加 `TopLevelAccessModifier = "E0442"` |
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | `ParseCompilationUnit` 分派点拒绝顶层 private/protected |
| `src/compiler/z42c.semantics/tests/access-control/access_control_tests.z42` | MODIFY | E0442 正/反用例（与②共文件） |

### ④ 接口类型可见性（compiler）
| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42InterfaceType` 加 `Visibility` 字段 |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `_passInterfaces` 读 `c.Mods` 设可见性 |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | `_interfaceDesc` 设 `cd.Visibility` |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 接口导入路径传播可见性 |
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | MODIFY | `CheckTypeRef` 加 `Z42InterfaceType` 分支（与②共文件） |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedInterfaceZ` 加 `Visibility` 字段 |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `_rebuildInterface` 从 `cd.Visibility` 构造 |
| `src/tests/cross-zpkg/interface_internal_access/` | NEW | 跨包 internal 接口引用 → E0404 fixture |

### 文档
| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/design/language/access-control.md` | MODIFY | Phase 2 状态更新：四项补齐 |
| `docs/book/src/compiler/access-control.md` | MODIFY | 机制页：反射面/不一致可访问性/顶层拒绝/接口可见性 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 勾销四项 |
| `src/libraries/z42.core/README.md` | MODIFY | Type 反射功能索引补可见性谓词 |

**只读引用**：
- `src/compiler/z42c.semantics/src/IrGenFacts.z42` — `classVisCode/classVisStr`（可见性 byte↔string 编码，item④ 复用）
- `src/compiler/z42c.semantics/src/Symbol.z42` — `MethodSymbol/FieldSymbol.Visibility`（item② 比对源）
- `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` / `ZbcReader.z42` — 确认接口 TYPE 已写/读可见性字节（无需改）
- `src/compiler/z42c.semantics/src/SemanticDump.z42` — 确认 TypeChecker/parser bag 计数（测试可见性）

## Out of Scope
- **嵌套私有/保护类的完整 accessibility-domain 偏序**（C# 的 protected⊕internal 组合）：item② 用线性 rank 近似（见 design D2），完整偏序 Deferred。
- **`protected internal` / `private protected` 组合修饰符**：规范明确不支持（E0405 已拒），不引入。
- **成员级不一致可访问性以外的可访问性传播**（如泛型约束暴露）：Deferred。
- **格式 bump**：本 change 不触发（#184 字节已在线）。

## Open Questions
- 无（三处设计分叉已由 User 裁决：一个 PR/4 commit、反射全 4 级对齐 C#、顶层拒绝覆盖类/接口/枚举/函数）。
