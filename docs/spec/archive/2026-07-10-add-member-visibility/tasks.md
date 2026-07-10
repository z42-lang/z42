# Tasks: 成员可见性元数据（P1-b）

> 状态：🟢 已完成 | 创建：2026-07-09 | 完成：2026-07-10 | initiative: unify-type-metadata P1-b

## 进度概览
- [x] 阶段 1: z42c（IrFieldDesc/IrFunction.Visibility + IrGen 填 + ZbcWriter TYPE/SIGS）
- [x] 阶段 2: Rust（FieldDesc/FuncSig.visibility + read + TypeDesc + loader）
- [x] 阶段 3: 反射（FieldInfo/MethodInfo IsPublic/IsPrivate）
- [x] 阶段 4: 版本 bump + regen fixtures + golden hex
- [x] 阶段 5: 测试 + 全 GREEN + 自举不动点
- [x] 阶段 6: 文档 + 归档

## 阶段 1: z42c
- [x] 1.1 IrModule: IrFieldDesc.Visibility(int,默认 0) + IrFunction.Visibility
- [x] 1.2 IrGen: 填字段/方法 visibility（_visCode(mods)：public/private/protected/默认 public）
- [x] 1.3 ZbcFormat Minor 22→23
- [x] 1.4 ZbcWriter BuildType 字段块（实例+静态）写 visibility u8；WriteSigEntries is_static 后写 visibility
- [x] 1.5 **z42c 读侧对称**（关键补漏）：ZbcReader.z42（字段块 + SIGS）+ ZpkgReader.z42（ReadModuleSigs SIGS）消费 visibility——否则编译期读 dep zpkg 时游标错位（gen1-stdlib 编 z42.encoding 崩：非 gated 格式必须写读同步）

## 阶段 2: Rust
- [x] 2.1 bytecode FieldDesc.visibility:u8 + Function.visibility
- [x] 2.2 zbc_reader read_type 两字段块读 visibility；read_sigs is_static 后读；bump 常量
- [x] 2.3 FuncSig.visibility + types.rs FieldSlot.visibility + Function.visibility；loader/dispatch/reader 线程

## 阶段 3: 反射
- [x] 3.1 reflection.rs FieldInfo（build_field_info）/ MethodInfo（build_method_info + resolve_func_sig）塞 IsPublic/IsPrivate slot
- [x] 3.2 FieldInfo.z42 / MethodInfo.z42 IsPublic + IsPrivate

## 阶段 4: bump + regen
- [x] 4.1 cargo build debug+release
- [x] 4.2 两代自举（0.26 老种子 → 0.27）——gen1-stdlib 用 EMPTY Z42_LIBS（非 gated 格式，新 reader 只碰 0.27 兄弟，运行期 0.26 stdlib 走 entry-dir）
- [x] 4.3 regen zbc-format 6 + zpkg-format 4（packed×2/indexed/sym-only=.zsym）+ golden hex（empty/f5/selfcheck）+ zpkg header pin + Rust pinned(23/27) + ZpkgWriter Minor + expected.json minor

## 阶段 5: 测试 + GREEN
- [x] 5.1 z42c 单测 golden（empty/f5/selfcheck 已更）；Rust read 往返 + pinned（cargo lib 782+21 + zbc_compat 3 全绿）
- [x] 5.2 z42.core/tests/reflection.z42 test_field_visibility + test_method_visibility [Test]
- [x] 5.3 全 GREEN：xtask test ✅（e2e 197/0 + cross-zpkg 3/0 + stdlib [Test] 含 reflection vis + compiler 单测/f5 + 自举不动点 7/7 gen1==gen2 + vscode）；cargo lib 782+21 + zbc_compat 3/3

## 阶段 6: 文档 + 归档
- [x] 6.1 zbc.md/zpkg.md changelog + version-bumping 常量表 + reflection.md（成员可见性反射节 + 成员表）
- [x] 6.2 归档 + ACTIVE.md 释放三锁 + commit/push + 盯 CI

## 备注
- initiative P1-b；TSIG 可见性不删（P3）。非 gated：每字段/函数 +1 字节。
- 复用 Change A/P1-a 两代自举流程（0.26 release 老 VM 存下当种子）。
- **踩坑（1.5）**：只改 writer 忘 z42c 读侧 → gen1 z42c 编 stdlib 时 ReadModuleSigs 游标错位崩（array index out of bounds）。非 gated 格式变更：**writer + 全部 reader（Rust + z42c ZbcReader + z42c ZpkgReader）必须同一提交对称改**。P1-a enum 是 gated（旧 zpkg 无 enum block → 读侧跳过）故没暴露此坑。
- **两代自举非 gated 要点**：gen1 z42c（新 reader）不得读任何旧格式 zpkg；gen1-stdlib 的 Z42_LIBS 置空（兄弟从 per-member 0.27 dist 解析），运行期旧 stdlib 只经 entry-dir 供 VM 加载。
