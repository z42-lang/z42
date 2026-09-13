# Tasks: `ObjNew.CtorKnown`（zbc 1.39 / zpkg 0.44）

## 实现

- [x] `IrInstrObject.z42`：`ObjNewInstr.CtorKnown` + `Dump()` + `Clone()`
- [x] `ZbcInstr.z42` / `ZbcReaderInstr.z42`：尾部 u8 写/读对称
- [x] `CtorKnownFixup.z42`（新）：本包已发射函数全集 ∪ `Deps.Statics`（**完整 FQ**）→ 重算全部站点
- [x] `PackageCompile.z42` ⑩ 装配点接线
- [x] Rust：`ObjNewInsn.ctor_known` + `instr_decode.rs` 解码
- [x] Rust：`symres.rs` 判据（抽出纯函数 `ctor_missing_is_definite`，并集不替换）
- [x] Rust：interp（`exec_instr.rs` / `exec_object.rs`）+ JIT（`translate/object.rs` /
      `helpers/registry.rs` 签名 / `helpers/object.rs`）两后端贯通

## 格式 bump（version-bumping.md 步骤 1–9）

- [x] 1. `ZbcFormat.z42` Minor 38→39 + 常量旁注释
- [x] 2. `zbc_reader/versions.rs` `ZBC_VERSION_MINOR` + changelog 注释块
- [x] 3. `docs/design/runtime/zbc.md` Minor changelog 加 1.39 行
- [ ] 4. regen `src/tests/zbc-format/*/source.zbc`（6 个）—— **需新格式工具链**
- [ ] 5. z42c golden hex 单测 `zbc_tests.z42` 内嵌 hex 重截 —— **需新格式工具链**
- [x] 6. `ZpkgWriter.z42` Minor 43→44
- [x] 7. `zbc_reader/versions.rs` `ZPKG_VERSION_MINOR` + changelog
- [x] 8. `docs/design/runtime/zpkg.md` Minor changelog 加 0.44 行
- [ ] 9. regen `src/tests/zpkg-format/*/source.zpkg`（4 个，手工）—— **需新格式工具链**
- [x] `.claude/rules/version-bumping.md` 版本常量表同步（1/39、0/44）

## 门

- [x] `symres_tests.rs`：四条纯判据单测（含「未置位 + 零实参必须放行」）
- [x] cross-zpkg fixture ×3（`_skew` / `_present` / `_absent`）+ README 登记
- [ ] **退回对照**：`_skew` 在未改的编译器上必须 FAIL（打印 `constructed 0`）
- [ ] `xtask test e2e --dir cross-zpkg` 两后端（含 `--mode jit`）
- [ ] 对账：复跑普查探针，确认置位面与预期一致（imported 侧 5 个站点不置位）

## GREEN（走 version-bumping.md「本地全量验证」配方）

- [ ] 推 PR 第一轮 → 取 `compile-toolchain` 的 `toolchain-macos-15` artifact
- [ ] overlay 成本地种子（保留自己的 cargo `z42vm`）+ `Z42_PORTABLE_VM`
- [ ] `xtask build compiler && xtask build stdlib && xtask build test`
- [ ] `cargo test`（含 `zbc_compat` / `format_fixture_versions` / `lazy_loader`）
- [ ] `xtask test` 完整 GREEN + 自举字节不动点 gen1 == gen2
- [ ] 冷构建复验（编译器类改动，规范 §3.1）
- [ ] `xtask test bootstrap`

## 收尾

- [x] book 机制页 `runtime/missing-symbol-resolution.md` 改写构造器那一节
- [ ] 归档 `changes/` → `archive/`（随 PR 同一提交）
- [ ] memory `dep-version-skew-program`「② 的墙」改写为已闭合
