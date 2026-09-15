# Spec: 跨包关联类型

## ADDED Requirements

### Requirement: 关联类型三份数据经 wire 跨包承载

#### Scenario: 接口关联类型名单跨包恢复
- **WHEN** 包 A 定义 `interface IEnum { type Item; }`，包 B `using A` 引用 IEnum
- **THEN** 包 B 编译时导入的 `Z42InterfaceType.AssocTypeNames` = `["Item"]`（经 TYPE assoc 块恢复）

#### Scenario: 类侧绑定跨包恢复
- **WHEN** 包 A 定义 `class IntBag : IEnum { type Item = int; }`，包 B 引用 IntBag
- **THEN** 包 B 导入的 `Z42ClassType.AssocBindingOf("Item")` = `"int"`

#### Scenario: where 约束绑定跨包恢复
- **WHEN** 包 A 定义 `class Holder<T> where T : IEnum<Item = int>`，包 B `new Holder<IntBag>()`
- **THEN** 包 B 拿到 Holder 约束里的 `AssocBinding("Item", "int")`（经 bit7 恢复），校验 IntBag 满足

### Requirement: 跨包关联类型完整校验（删三守卫后）

#### Scenario: 跨包正确绑定放行
- **WHEN** 包 B `new Holder<IntBag>()`，IntBag 绑 `Item=int`、约束要求 `Item=int`
- **THEN** 无诊断，编译运行正常（`assoc_type_cross_pkg` 输出 7/9/11）

#### Scenario: 跨包错绑定报错
- **WHEN** 包 B 用绑定 `Item=string` 的导入类满足 `where T:IEnum<Item=int>`（或干脆不绑）
- **THEN** 报 E0453（绑定不符 / 缺绑定）

## MODIFIED Requirements

### Requirement: 关联类型满足性不再豁免跨包

**Before:** `ConstraintChecker._fillBundle`(:169) / `_checkAssocBinding`(:416) / `InheritanceResolver._checkOneIfaceAssoc`(:360) 遇 `IsImported` 早退，跨包关联类型不校验（保守漏报，防假红）。

**After:** 删三守卫。跨包与同包一样接受完整关联类型校验（导入侧 AssocTypeNames/AssocBinding 已由 wire 恢复）。

## IR Mapping

- zbc 1.42：① 约束 bundle bit7（`assoc_count:u8 + (name,type)×n`，iface 列表后）；② TYPE 记录尾部 assoc 块（`assoc_count:u16 + (name,type)×n`，always-present）。
- zpkg 0.47：内嵌 zbc 1.42。

## Pipeline Steps

- [x] Parser/AST —（无；`type Item;` / `type Item=int` / `<Item=int>` 已解析）
- [ ] TypeChecker — 删三守卫，跨包校验生效
- [x] IR Codegen — ClassDescBuilder 填 assoc 字段；ZbcWriter/Reader/ZpkgReader/TsigReconcile/ImportedSymbolLoader 搬运
- [x] VM interp — type_reader.rs 读 assoc 字节（消费对齐，行为不变）
