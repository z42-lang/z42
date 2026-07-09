# Tasks: enum 类型元数据（P1-a）

> 状态：🟢 已完成 | 创建：2026-07-09 | 完成：2026-07-09 | initiative: unify-type-metadata P1-a

## 进度概览
- [ ] 阶段 1: z42c IR + codegen（enum→IrClassDesc→TYPE 成员块）
- [ ] 阶段 2: Rust reader + TypeDesc.is_enum + 成员
- [ ] 阶段 3: 反射 API（Type.IsEnum + Std.Enum.*）
- [ ] 阶段 4: 版本 bump + fixture regen + golden
- [ ] 阶段 5: 测试 + 全 GREEN + 自举不动点
- [ ] 阶段 6: 文档 + 归档

## 阶段 1: z42c
- [ ] 1.1 `IrModule.z42` `IrClassDesc` 加 `IsEnum` + `EnumMemberNames`/`EnumMemberValues`
- [ ] 1.2 `IrGen.z42` EnumDecl → IrClassDesc(enum),填成员名+值(复用 enum 常量求值)
- [ ] 1.3 `SymbolCollector.z42` enum 登记类符号(供 typeof),保留 EnumConsts 映射
- [ ] 1.4 `ZbcFormat.z42` `CLASS_FLAG_ENUM=0x20` 常量 + `ZbcVersion.Minor` bump
- [ ] 1.5 `ZbcWriter.InternPoolStrings` 预扫 enum 成员名入池
- [ ] 1.6 `ZbcWriter.BuildType` 写 enum bit + 成员块(additive 尾部)

## 阶段 2: Rust reader
- [ ] 2.1 `bytecode.rs` `ClassDesc` 加 `enum_members`；启用 `CLASS_FLAG_ENUM`
- [ ] 2.2 `zbc_reader.rs` `read_type` 读 enum bit + 成员块；`ZBC/ZPKG_VERSION_MINOR` bump
- [ ] 2.3 `types.rs` `TypeDesc` 加 `is_enum` + `enum_members`(loader 组装)

## 阶段 3: 反射 API
- [ ] 3.1 `reflection.rs` `__type_is_enum` builtin + `__enum_names`/`__enum_values`/`__enum_name` builtin
- [ ] 3.2 `z42.core/src/Type.z42` `IsEnum` 属性
- [ ] 3.3 `z42.core/src/Enum.z42` `Std.Enum.GetNames/GetValues/GetName`

## 阶段 4: bump + regen
- [ ] 4.1 cargo build debug+release VM
- [ ] 4.2 两代自举建 0.x 工具链（bump → gen1/gen2）
- [ ] 4.3 regen zbc-format + zpkg-format fixture + golden hex
- [ ] 4.4 `ZpkgWriter.Minor` + zbc.md/zpkg.md changelog + version-bumping 常量表

## 阶段 5: 测试 + GREEN
- [ ] 5.1 z42c 单测：enum BuildType golden + typeof 解析
- [ ] 5.2 Rust 单测：read_type enum 往返 + version pinned
- [ ] 5.3 `src/tests/types/enum_reflect.z42` 端到端
- [ ] 5.4 `z42.core/tests/enum_metadata/` [Test]
- [ ] 5.5 全 GREEN + 自举不动点 + cargo metadata
- [ ] 5.6 spec 9 scenario 逐条覆盖

## 阶段 6: 文档 + 归档
- [ ] 6.1 zbc.md TYPE enum 块 + changelog；zpkg.md changelog
- [ ] 6.2 roadmap 0.3.12 IsEnum 勾掉一部分 + Deferred index（若有）
- [ ] 6.3 归档 + ACTIVE.md 释放三锁 + commit/push + 盯 CI

## 备注
- initiative unify-type-metadata P1-a；TSIG enum 块本 change **不删**(P3 才删)。
- format bump 走两代自举（Change A 已验证该路径可用）。

### 实施地图（2026-07-09 autonomous 勘查，供执行时直接照做）
- **老种子已快照**：`artifacts/.scratch/p1a/{seed-c(7 z42c 0.25),seed-l(22 stdlib 0.25),oldvm-release}`——
  两代自举用它当 gen0（bump 版本重建新 VM 前务必保住这份 0.25 VM）。
- **IrClassDesc**（`z42c.ir/src/IrModule.z42:42`）：现有 Name/HasBase/BaseName/Fields/StaticFields/
  Interfaces/**Flags**(bit0 abstract/1 sealed/2 struct/3 record)/TypeParams/Constraints/Attrs。
  加 `EnumMemberNames:string[] + EnumMemberValues:long[] + EnumMemberCount:int`；enum 置 **Flags bit5(0x20)**。
- **IrGen**（`z42c.semantics/src/IrGen.z42:124-138`）：现只从 `ClassDecl` 建 IrClassDesc；**加 `d is EnumDecl`
  分支** → 建 enum IrClassDesc(Flags|0x20 + 成员)。EnumDecl AST(`Decl.z42`)= Mods/Name/HasBase/Base/
  Members(EnumMember[])/MemberCount。**成员值求值逻辑**在 `SymbolCollector._passEnums`（填 EnumConsts 名→值,
  含显式 `=N`/隐式递增)——执行前先读它,复用同款求值(勿重造)。
- **SymbolCollector**：enum 额外登记一个类符号(Z42ClassType 标 enum)供 `typeof(EnumType)` 解析；
  EnumConsts 映射保留不动。
- **ZbcFormat.z42**：`ZbcVersion.Minor` 21→22 + 注释；z42c 无 CLASS_FLAG 常量,直接 `Flags | 0x20`。
- **ZbcWriter**：`InternPoolStrings` 类段预扫处加 enum 成员名入池；`BuildType`(:192) 写 class_flags 时
  含 bit5,若 enum 在类记录**尾部**(attributes 之后)追加 `member_count:u16 + (name_idx:u32,value:i64)×n`。
- **Rust**：`bytecode.rs:129 ClassDesc` 加 `enum_members:Box<[(String,i64)]>`；`CLASS_FLAG_ENUM` 已在
  `bytecode.rs:125` 注释预留 bit5,定义常量并用；`zbc_reader.rs:373 read_type` 读 bit5 → 读成员块;
  `ZBC_VERSION_MINOR 21→22`+`ZPKG_VERSION_MINOR 25→26`(旁 changelog);`types.rs TypeDesc` 加
  `is_enum:bool + enum_members`；`loader.rs` 组装 TypeDesc 时填。
- **反射**：`reflection.rs` 加 `__type_is_enum`/`__enum_names`/`__enum_values`/`__enum_name` builtin(读
  TypeDesc.enum_members)；`z42.core/src/Type.z42` `IsEnum`;新 `z42.core/src/Enum.z42` `Std.Enum.GetNames/
  GetValues(int[])/GetName`。
- **ZpkgWriter.z42** Minor 25→26。
- **两代自举 recipe**：同 Change A design D7——`cd src/compiler; Z42_LIBS=seed-l oldvm seed-c/z42c.driver.zpkg
  -- build --workspace --release` → gen1；组 g1run(gen1 7 包+seed-l)；gen1-stdlib；组 flat26；gen2 z42c。
  然后新 VM 建 xtask、regen、GREEN。**坑**：debug+release VM 都要 rebuild；golden hex 多处(empty/f5/
  selfcheck/zpkg header)重截；loader_tests/zbc_reader_tests pinned 常量;zpkg-format 4 fixture 手工 regen。
- **验证 enum 常量不回归**：`Direction.North`==0 编译路径 + 现有 enum golden 保持。
