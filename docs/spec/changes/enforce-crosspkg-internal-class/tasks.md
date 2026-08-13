# Tasks: 跨包 internal 类引用强制（类可见性序列化，格式 bump）

> 状态：🟡 实施中（本地已应用 + 编译/cargo 验证；格式-bump GREEN 走 CI）| 创建：2026-08-13
> 分支：`enforce-crosspkg-internal-class` | worktree：`../z42-crosspkg-internal`（基于 origin/main，含 ① #183）
> 承接：① `enforce-class-access`（#183）的 D5/D6。②完整代码来自 patch，① 已合入 main。

## 进度概览
- [x] 阶段 1: 类可见性进 IR + zbc/zpkg 序列化（格式 bump zbc1.33/zpkg0.38）
- [x] 阶段 2: 本地设值（ClassDescBuilder）+ 跨包还原（ImportedSymbolLoader）激活 ① internal deny
- [x] 阶段 3: Rust VM read-and-discard + 版本 pin + struct-byte 单测
- [~] 阶段 4: 本地验证（macOS warm 0.37 上限：gen0 编译 + cargo build/test）
- [ ] 阶段 5: 跨包 e2e 测试
- [ ] 阶段 6: CI 两代自举 GREEN + 临时 fixture-regen 步 + 文档同步 + 归档

## 阶段 1: 序列化载体（z42.ir）
- [x] 1.1 `IrModule.z42`：`IrClassDesc` 加 `int Visibility`（默认 0）
- [x] 1.2 `ZbcWriter.z42`：`w.WriteU8(cd.Visibility)` 紧随 `WriteU8(cd.Flags)`
- [x] 1.3 `ZbcReader.z42`：`cd.Visibility = c.U8()` 紧随 `cd.Flags = c.U8()`
- [x] 1.4 `ZbcFormat.z42`：`Minor` 32→33 + changelog
- [x] 1.5 `ZpkgWriter.z42`：`Minor` 37→38 + changelog
- [x] 1.6 `ExportedTypes.z42`：`ExportedClassZ` 加 `string Visibility`（默认 "public"）
- [x] 1.7 `TsigReconcile.z42`：`ecz.Visibility = _visStr(cd.Visibility)`（_visStr 已支持 3→internal）

## 阶段 2: 本地设值 + 跨包还原（z42c.semantics）
- [x] 2.1 `ClassDescBuilder.z42`：`cd2.Visibility = IrGenFacts.classVisCode(c.Mods, c.Name.IndexOf("+")>=0)`
- [x] 2.2 `ImportedSymbolLoader.z42`：`nct.Visibility = cl.Visibility`（激活 ① `CheckTypeRef` internal deny）

## 阶段 3: Rust VM（读而不用）
- [x] 3.1 `zbc_reader.rs`：TYPE 段 `let _class_visibility = c.read_u8()?`（read-and-discard）
- [x] 3.2 `zbc_reader.rs`：`ZBC_VERSION_MINOR` 32→33 + `ZPKG_VERSION_MINOR` 37→38 + changelog
- [x] 3.3 `zbc_reader_tests.rs`：版本 pin + `build_type_section_one_struct` push 可见性字节

## 阶段 4: 本地验证（macOS 上限）
- [x] 4.1 z42.ir gen0 编译（seed 0.37 编 ② 源过）
- [x] 4.2 `cargo build`（z42vm）通过
- [x] 4.3 `cargo test --lib`：919 pass；5 committed-fixture 测试 + 11 host 集成测试因 0.38 stdlib 本地不可产而失败（macOS 两代自举墙，转 CI）
- [ ] 4.4 （CI）完整 GREEN

## 阶段 5: 跨包 e2e
- [ ] 5.1 `src/tests/cross-zpkg/class-internal-access/`：A 包 `internal class Secret` + B 包 `new Secret()` → 期望 E0404；harness 无 expected-error 模式则手工验 + 记录

## 阶段 6: CI + 文档 + 归档
- [ ] 6.1 push PR → 盯 `ci-bootstrap` 版本差 gate 两代自举建 0.38
- [ ] 6.2 加**临时 CI 步**重生 committed fixture（zbc-format×6 / zpkg-format×4 / empty source.zbc hex）并回写
- [ ] 6.3 CI 完整 GREEN → 合并 → 删临时步 + 删分支/worktree
- [ ] 6.4 `docs/design/language/access-control.md`：跨包 internal 移出 Deferred
- [ ] 6.5 doc-check + 归档 mv → `docs/spec/archive/2026-08-13-enforce-crosspkg-internal-class/`

## 备注
- **格式 bump 本地不可验**：macOS 两代自举墙（seed 本地产坏 gen0 stdlib，CI/Linux 正常），见 memory
  `escape-stack-format-bump-ci-learnings` / `add-crosspkg-internal-class`。GREEN 判定以 CI 为准。
- **破坏面≈0**：所有导出类 public → 可见性字节恒 0 → 自举 gen1==gen2 保持。
- ⚠️ cargo VM 缓存旧二进制——bump 后 `touch src/runtime/src/metadata/zbc_reader.rs` 强制重建，否则旧 0.37 VM 报 strict-pin。
