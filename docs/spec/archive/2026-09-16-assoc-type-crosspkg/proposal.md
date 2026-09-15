# Proposal: 跨包关联类型（associated types across zpkg）

## Why

关联类型（`interface IEnum { type Item; }` + `where T : IEnum<Item = int>` + 实现方
`class IntBag : IEnum { type Item = int; }`）**同包**已支持（`add-associated-types` 已归档），
但**跨包不通**——三份语义数据一个字节都没进 zbc/zpkg wire：

1. 约束侧绑定 `where T:IEnum<Item=int>` 的 `Item=int`（`ConstraintBundle.AssocBinding*`）
2. 接口关联类型名单 `type Item;`（`Z42InterfaceType.AssocTypeNames`）
3. 类侧绑定 `type Item=int;`（`Z42ClassType.AssocBinding*`）

导入侧这三份恒空 ⇒ 合法跨包代码会被误判红。`add-associated-types` PR-3 因此留了**三处临时
`IsImported` 守卫**（`ConstraintChecker:169/416`、`InheritanceResolver:360`）保守跳过跨包校验
（stopgap）。本 change 让三份数据经 wire 承载，删三守卫，跨包关联类型获**完整判别力**
（与同包一致：正确绑定放行、错绑定/缺绑定报 E0453）。

## What Changes

- **zbc/zpkg 双格式 bump**（zbc 41→42、zpkg 46→47）——bit7 + TYPE 块新增，三方 reader 需同步。
- **wire 新增 2 处**：
  - **约束 bundle bit7**（`has_assoc_binding`）：`assoc_count:u8 + (name_idx:u32, type_idx:u32)×n`，
    承载 `where` 约束里的关联类型绑定（供导入泛型类型的实例化校验）。
  - **TYPE 记录统一 assoc 块**（always-present，记录尾部）：`assoc_count:u16 + (name:u32, type:u32)×n`。
    接口写 `(Item, "")` = AssocTypeNames；类写 `(Item, int)` = AssocBinding。消费端按接口/类路由：
    接口 → `AddAssocType(name)`、类 → `AddAssocBinding(name, type)`。**一个块承载接口名单 + 类侧绑定**。
- `IrConstraintDesc` 加 `AssocBindingNames[]/TypeNames[]/Count`；`IrClassDesc` 加 `AssocNames[]/AssocTypes[]/AssocCount`。
- z42c writer（ZbcWriter）写 bit7 + TYPE assoc 块；三方 reader（ZbcReader / runtime type_reader.rs /
  ZpkgReader）对称读（runtime + ZpkgReader 只消费字节保游标对齐——关联类型纯编译期，
  `validate_type_arg_constraint` 无关联类型分支）。
- `ClassDescBuilder` 填 IrClassDesc/IrConstraintDesc 的 assoc 字段。
- `ExportedInterfaceZ` 加 `AssocTypeNames[]`；类侧绑定 + 约束绑定经 IrClassDesc/IrConstraintDesc 承载。
- `TsigReconcile._rebuildClass/_rebuildInterface` 搬运 assoc 数据；`ImportedSymbolLoader` 读回
  （`AddAssocType` / `AddAssocBinding` / 约束 `b.AddAssocBinding`）。
- 删三处 `IsImported` 守卫 → 导入接口/类获完整关联类型校验。
- `assoc_type_cross_pkg` fixture 升级：正例仍跑 7/9/11，新增负例（跨包错绑定 → E0453）。
- 格式 fixture（zbc-format 6 + zpkg-format 4 + golden hex）重生 + 版本常量四处。

## Scope（允许改动的文件）

| 文件 | 类型 | 说明 |
|---|---|---|
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | IrConstraintDesc + IrClassDesc 加 assoc 字段 + ctor 初始化 |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 填 IrClassDesc/约束的 assoc 字段 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 写 bit7 + TYPE assoc 块 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | 读 bit7 + TYPE assoc 块 |
| `src/libraries/z42.ir/src/ZpkgReader.z42` | MODIFY | bit7 + assoc 块消费保对齐 |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | ExportedInterfaceZ 加 AssocTypeNames |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | _rebuildClass/_rebuildInterface 搬运 assoc |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 读回接口/类/约束 assoc |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | ZbcVersion.Minor 41→42 |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | ZpkgWriterZ.Minor 46→47 |
| `src/runtime/src/metadata/zbc_reader/type_reader.rs` | MODIFY | 读 bit7 + TYPE assoc 块（消费对齐） |
| `src/runtime/src/metadata/zbc_reader/versions.rs` | MODIFY | ZBC 42 + ZPKG 47 + changelog |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | 版本断言 41→42/46→47 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | 删 2 守卫（:169/:416） |
| `src/compiler/z42c.semantics/src/InheritanceResolver.z42` | MODIFY | 删 1 守卫（:360） |
| `src/tests/cross-zpkg/assoc_type_cross_pkg/**` | MODIFY | 升级：正例 + 新负例 |
| `src/tests/zbc-format/*/source.zbc` | MODIFY | 6 fixture 重生 |
| `src/tests/zpkg-format/*/source.zpkg` | MODIFY | 4 fixture 重生 |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | golden hex |
| `docs/design/runtime/zbc.md` / `zpkg.md` | MODIFY | changelog 1.42 / 0.47 |
| `docs/book/src/language/generics.md` | MODIFY | 跨包关联类型章节 |

**只读引用**：`Z42Type.z42`（AssocType/AssocBinding 语义结构）、`MemberCollector.z42`（同包填充路径）。

## Out of Scope

- **B4 带实参约束匹配**（`where T:IEnumerable<int>` 比实参）、**B5 传递约束**（`Item=U, U:IDisplay`）——独立 change。
- runtime 侧关联类型校验（VM 不需要，纯编译期）。

## Open Questions

无（设计已锁：bit7 + TYPE 统一 assoc 块）。
