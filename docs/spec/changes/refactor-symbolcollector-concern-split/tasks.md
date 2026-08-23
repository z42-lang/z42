# Tasks: SymbolCollector 按职责分解为 hub + 4 收集/校验簇

**变更说明：** 把 1158 行 god-class `SymbolCollector` 按职责拆为 hub（编排入口 + imported 种子 + 共享辅助）+ 4 个簇（StubCollector / MemberCollector / InheritanceResolver / DeclEnforcer），全文件 < 500 行。
**原因：** god-class 超 500 硬限；且按真实职责分离便于后续扩展（加声明种类 / 基链规则 / 后缀 kind 各自局部化）。纯结构搬移，stdlib 逐字节不变。
**文档影响：** `z42c.semantics/README.md`（功能索引 + 核心文件）；`compiler-structure-refactor-program` memory。

- [ ] 1.1 StubCollector 簇：_passInterfaces/_passEnums/_parseEnumVal/_passClassStubs/_putClassStub/_ctHasIface/_passDelegates
- [ ] 1.2 MemberCollector 簇：_passMembers/_fillInterface/_fillClass/_collectConstField/_isPureRef/_partialSigKey
- [ ] 1.3 InheritanceResolver 簇：_passFixupOverrides/_findVirtualOrigin/_passSealedEnforce/_nearestBaseMethod/_passInheritFields/_passImpls
- [ ] 1.4 DeclEnforcer 簇：suffix enforce ×3 + _baseIsAttribute/_baseHasSimpleName + _passValidatePartial/_checkNestedPartial/_passResolvePartialMethods
- [ ] 1.5 hub 保留：3 入口 + imported 种子 + 共享辅助（_unwrap/_vis/_hasWord/_chkTypeRef/_methodSymbol/_mergeParams/IsProtocolExempt/_isConvOp）+ spoke 字段/构造/internal
- [ ] 1.6 逐字节守卫：build compiler + build stdlib → stdlib 24/24 shasum == baseline
- [ ] 1.7 GREEN（完整 xtask test，含 self-host 5/5 gen1==gen2）
- [ ] 1.8 文档同步：README 六段 + memory
