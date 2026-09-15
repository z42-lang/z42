# Tasks: 接口方法 static 保真度

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-16 | PR #681

## 进度概览
- [x] 阶段 1: wire 承载（IrClassDesc + writer/reader + ClassDescBuilder + TsigReconcile）
- [x] 阶段 2: runtime reader（type_reader + IfaceMethodSig）
- [x] 阶段 3: 格式 bump（4 处版本常量 + changelog）
- [x] 阶段 4: 删守卫 + 满足性校验对导入接口生效
- [x] 阶段 5: 测试（负例 cross-zpkg E0412 复现 + 正例 Money:INumber 编译运行绿）
- [x] 阶段 6: fixture 重生（zbc 6 + zpkg 4 + golden hex 2 处，用 CI 46 工具链）
- [x] 阶段 7: 文档同步 + 归档

## 验证结果（2026-09-16）
- Rust：编译通过 + 版本 pin 41/46 + zbc_compat 3/3（zbc-format）+ lazy_loader 32/32（zpkg-format）
- 逻辑（本地 46 工具链）：负例 `iface_static_impl_mismatch` main 编不过、精确 E0412
  「`MakeZero` is `static` in the interface and an instance method here」；正例
  `static_abstract_operator.z42`（Money:INumber）编译零 E0412 + 运行 rc=0
- CI：compile-toolchain(linux-x64+macos-arm64) ✅、stdlib-interp/jit ✅、desktop-cabi ✅、bench ✅；
  golden/fixture 相关 stage 首轮红（stale 45），本次二推重生后转绿
- 🔴 macOS 本地两代自举墙实锤（`build compiler` 用 cargo VM，格式 bump 45/46 混态无法本地收敛）
  ⇒ 格式 fixture 走「下载 CI `toolchain-macos-26` artifact（46 driver+stdlib，zpkg 平台无关）
  + 本地 cargo 46 VM」重生 —— 比 escape-stack 记的临时 CI 步骤更省，下次格式 bump 照此

## 阶段 1: wire 承载
- [ ] 1.1 `IrModule.z42` `IrClassDesc` 加 `int[] IfaceMethodStatic` + ctor 初始化 `new int[0]`
- [ ] 1.2 `ClassDescBuilder.z42` 建接口方法块时填 `ims[imc] = _hasWord(imd.Mods,"static")?1:0`（含数组增长同步）
- [ ] 1.3 `ZbcWriter.z42` 接口方法块 `pcount` 后写 `WriteU8(cd.IfaceMethodStatic[imw])`
- [ ] 1.4 `ZbcReader.z42` 接口方法块 `pcount` 后读 `is_static`，填 `cd.IfaceMethodStatic[]`
- [ ] 1.5 `TsigReconcile._rebuildInterface` 用真值构造 ExportedMethodZ（isStatic/!isStatic/true）

## 阶段 2: runtime reader
- [ ] 2.1 `class.rs` `IfaceMethodSig` 加 `is_static: bool`
- [ ] 2.2 `type_reader.rs` 接口方法块 `pcount` 后读 `is_static` 字节，填入 IfaceMethodSig

## 阶段 3: 格式 bump
- [ ] 3.1 `ZbcFormat.z42` Minor 40→41 + 注释
- [ ] 3.2 `ZpkgWriter.z42` Minor 45→46 + 注释
- [ ] 3.3 `versions.rs` ZBC_VERSION_MINOR 41 + ZPKG_VERSION_MINOR 46 + 两处 changelog 追加行
- [ ] 3.4 `zbc_reader_tests.rs` 版本断言 40→41 / 45→46

## 阶段 4: 删守卫
- [ ] 4.1 `InheritanceResolver.z42` 删 `if (it.IsImported) { return; }`（line 441）+ 更新注释块

## 阶段 5: 测试
- [ ] 5.1 z42c 接口满足性单元测试（阳性 + 故意违反报 E0412）
- [ ] 5.2 `src/tests/cross-zpkg/iface_static_cross_pkg/` e2e（target static-abstract 接口 + main 跨包实现调用）
- [ ] 5.3 退回对照：注释掉守卫删除 / 回退 TsigReconcile → 精确条数变红
- [ ] 5.4 Rust reader roundtrip / 版本断言

## 阶段 6: fixture 重生
- [ ] 6.1 zbc-format 6 fixture（`xtask build test` 或 CI escape hatch）
- [ ] 6.2 zpkg-format 4 fixture（手工重生）
- [ ] 6.3 golden hex（`zbc_tests.z42` empty header minor）
- [ ] 6.4 `cargo test --test zbc_compat` / `lazy_loader`

## 阶段 7: 验证 + 文档 + 归档
- [ ] 7.1 完整 `xtask test`（interp）+ stdlib/cross-zpkg jit + `test bootstrap`
- [ ] 7.2 `docs/design/runtime/zbc.md` + `zpkg.md` changelog
- [ ] 7.3 `docs/book/src/language/generics.md` 接口满足性覆盖导入接口
- [ ] 7.4 归档（changes→archive + tasks 🟢）+ PR

## 备注
- 格式 bump：macOS 本地两代自举有环境墙，完整 GREEN 以 CI 为准（`ci-bootstrap` 版本差 gate → 两代自举）。
- ExportedMethodZ ctor 元数不变（isStatic/isVirtual/isAbstract 是既有 args 4/5/6）。
- ⚠️ **可见性收紧连带修复（round-3）**：删守卫让「接口实现必须显式 public」（#651 严格口径，原只覆本包——
  #651 曾误以为导入侧 Visibility 不可靠，实则 cm.Visibility 查本地实现方、恒可靠）扩展到**导入接口**。
  全仓实测唯一 violator = `src/tests/generics/generic_interface_dispatch.z42`（`IntEqComparer`/`IntDescComparer`
  的接口成员漏写 public）→ 加 public（对齐 C# 隐式-public 规则；编译+运行 rc=0 保行为）。stdlib/compiler
  （compile-toolchain 已过）+ 全部 cross-zpkg 接口 fixture 实测零 violator。
  [[silent-feature-masks-other-bugs]] 一例：删守卫同时启用 static+可见性+返回三项，**可见性爆炸半径比
  static 广**，须全量扫（`--emit-zbc` 收集「must be implemented public」+ 多包 cross-zpkg 逐链单独编）。
