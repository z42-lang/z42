# Tasks: 跨包关联类型

> 状态：🟡 进行中 | 创建：2026-09-16

## 进度概览
- [ ] 阶段 1: IR 结构（IrConstraintDesc + IrClassDesc assoc 字段）
- [ ] 阶段 2: ClassDescBuilder 填充
- [ ] 阶段 3: wire 写（ZbcWriter：bit7 + TYPE assoc 块）
- [ ] 阶段 4: wire 读（ZbcReader + ZpkgReader + runtime type_reader）
- [ ] 阶段 5: 搬运（ExportedInterfaceZ + TsigReconcile + ImportedSymbolLoader）
- [ ] 阶段 6: 格式 bump（4 版本常量 + changelog）
- [ ] 阶段 7: 删三守卫
- [ ] 阶段 8: 测试（fixture 升级 + 单元 + 退回对照）
- [ ] 阶段 9: fixture 重生（zbc 6 + zpkg 4 + golden hex）+ 文档 + 归档

## 阶段 1: IR 结构
- [ ] 1.1 `IrConstraintDesc` 加 `AssocBindingNames[]/TypeNames[]/Count` + ctor
- [ ] 1.2 `IrClassDesc` 加 `AssocNames[]/AssocTypes[]/AssocCount` + ctor

## 阶段 2: ClassDescBuilder
- [ ] 2.1 建 IrClassDesc 时：接口→AssocNames(type="")；类→AssocNames/AssocTypes（从 AssocTypeDecl bound）
- [ ] 2.2 建约束 bundle 时：填 IrConstraintDesc.AssocBinding*（从 where 的 AssocBindingType）

## 阶段 3: wire 写（ZbcWriter）
- [ ] 3.1 约束 bundle：bit7 gated `assoc_count:u8 + (name,type)×n`（iface 列表后）
- [ ] 3.2 TYPE 记录尾部：`assoc_count:u16 + (name,type)×n`（always-present）

## 阶段 4: wire 读
- [ ] 4.1 `ZbcReader._readConstraintBundle`：读 bit7 assoc → IrConstraintDesc
- [ ] 4.2 `ZbcReader` TYPE：读 assoc 块 → IrClassDesc
- [ ] 4.3 `ZpkgReader._skipConstraintBundle`：bit7 + assoc 块消费保对齐
- [ ] 4.4 `type_reader.rs`：bit7 + TYPE assoc 块消费（读而不存）

## 阶段 5: 搬运
- [ ] 5.1 `ExportedInterfaceZ` 加 `AssocTypeNames[]` + ctor 后赋值
- [ ] 5.2 `TsigReconcile._rebuildInterface`：填 AssocTypeNames；`_rebuildClass`：拷 IrConstraintDesc assoc + 类 assoc
- [ ] 5.3 `ImportedSymbolLoader`：接口 AddAssocType / 类 AddAssocBinding / 约束 b.AddAssocBinding

## 阶段 6: 格式 bump
- [ ] 6.1 `ZbcFormat.z42` 41→42 + 注释
- [ ] 6.2 `ZpkgWriter.z42` 46→47 + 注释
- [ ] 6.3 `versions.rs` ZBC 42 + ZPKG 47 + changelog
- [ ] 6.4 `zbc_reader_tests.rs` 断言 41→42/46→47

## 阶段 7: 删三守卫
- [ ] 7.1 `ConstraintChecker.z42:169` `!abIface.IsImported`
- [ ] 7.2 `ConstraintChecker.z42:416` `cls.IsImported`
- [ ] 7.3 `InheritanceResolver.z42:360` `it.IsImported`

## 阶段 8: 测试
- [ ] 8.1 `assoc_type_cross_pkg` 升级（正例 + 新负例 expected_build_error）
- [ ] 8.2 单元（跨包满足性正/负 + 退回对照）
- [ ] 8.3 Rust reader roundtrip + 版本 pin

## 阶段 9: 收尾
- [ ] 9.1 fixture 重生（下载 CI 46→47 工具链，同 B1 配方）
- [ ] 9.2 完整 GREEN（interp + jit + bootstrap，CI 为准）
- [ ] 9.3 文档（zbc/zpkg changelog + generics.md）
- [ ] 9.4 归档 + PR

## 备注
- 格式 bump 完整 GREEN 以 CI 为准（macOS 本地两代自举墙）。fixture 走「下载 CI toolchain artifact + 本地 cargo VM 重生」（B1 已验证配方）。
- 三方 reader（ZbcReader / type_reader.rs / ZpkgReader）必对称消费 bit7 + TYPE assoc 块，漏一个游标错位。
