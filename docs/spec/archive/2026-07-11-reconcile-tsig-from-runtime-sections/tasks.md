# Tasks: TSIG 对账重建（P2）

> 状态：🟢 已完成 | 完成：2026-07-11| 创建：2026-07-11 | initiative: unify-type-metadata P2

## 进度概览
- [x] 阶段 1: 读取面（ReadModuleSigs 灌元数据 + ReadModuleTypes + ZbcReader TYPE 公开）
  - ReadModuleSigs：visibility/method_flags/min_arg/params_from/参数名/默认值灌进 IrFunction stub
  - ZbcReader.ReadTypeAt 公开（_readType factor）；ZpkgReader.ReadModuleTypes（packed MODS 第②体）+ ZpkgModuleTypes
- [x] 阶段 2: TsigReconcile（Rebuild + 归一化 + Compare，world 跨包 base 链）+ driver verb `reconcile-tsig`
- [x] 阶段 3: 29 包对账全 OK ✅ + 单测（reconcile_tests.z42 归一化 4 断言）✅ + 全 GREEN ✅ + 不动点 7/7 ✅
  - **✅ 全 29 包（22 stdlib + 7 z42c）对账 100% OK**。收敛的归一化/根因修（累计）：
    ① 祖先 (pkg,mod) 精确定位（world 全包 TYPE 检索，替代 ns 首中）
    ② oracle 口径：enums 恒=内建 GCHandleType、Functions=全包列表每模块重复、HasBase=!isStruct（struct base ""）
    ③ 方法两 pass：实例链合并（override 替换祖先位 + IsVirtual:=false；abstract 不置 virtual）→ 本类 static append（IsVirtual 恒 false）
    ④ 排除：__static_init__ / __lambda / __local_
    ⑤ 归一化：internal≈public（无消费方门禁）、类型双拼写 canon（byte↔u8 等 + nullable 擦除 + 泛型实参擦除 + Predicate→Func）
    ⑥ 裸名剥 arity-mangle $N
    ⑦ IrGen 根因修：native 桩真实 ret/参数类型（原硬编码 object）、5 个合成点补逻辑 MinArg（property/indexer/synth-ctor）、auto-prop 后备字段 Visibility=private
    ⑧ 幽灵自由函数根因：`_bare` 在 `Name$N.Method` 处误剥（首个 `$` 截断）→ 拆 `_stripNs`（ns-only，free-func "." 检测用）vs `_bare`（尾部 arity-mangle 剥，仅 `$` 后全数字）
    ⑨ 排除隐式根 `Std.Object`（TSIG 不导出）
    ⑩ **子集语义**（oracle ⊆ rebuilt，按 name+isStatic+paramCount 键匹配）——**关键 P2 发现：TSIG 有损**（漏报属性 `private set`），SIGS 更全 → 删 TSIG 零信息损失、反而更完整；子集判据是「无信息丢失」安全网的正确 bar
  - **P2 结论：删 TSIG 安全**——rebuild(TYPE/SIGS/IMPL) ⊇ TSIG 消费面，逐字段（归一化后）相等
  - **顺带揪出 P1-d 潜伏 bug**：indexed writer `_internSigStrings` 漏 intern 参数名 + str 默认值
    （P1-d 只更新了 packed 的 InternPoolStrings）→ WriteSigEntries 的 name_str_idx 落 0xFFFFFFFF。
    P1-d/P1-e② 未暴露因 ReadModuleSigs 当时只 consume（跳字节不 deref）；P2 改 store-deref 后
    `test_indexed_main_roundtrip` 崩溃暴露 → 补 intern（镜像 packed）。self-host 不动点 7/7 保持。
- [x] 阶段 4: 文档（project.md 机制节 + z42c.project README + roadmap Deferred）+ 归档

## 备注
- TSIG=oracle 零行为变化；CI gate 布线待 toolchain 锁（fix-bootstrap 持有）→ follow-up。
- 归一化以实测差异逐项收敛（裸名/短基名/"p{i}"/可见性串/arity demangle/纳入规则）。
