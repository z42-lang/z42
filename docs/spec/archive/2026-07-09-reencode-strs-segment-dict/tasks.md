# Tasks: STRS segment-dict 重编码

> 状态：🟢 已完成 | 创建：2026-07-09 | 完成：2026-07-09

## 进度概览
- [x] 阶段 1: varint 原语（writer + 3 游标）
- [x] 阶段 2: STRS 编码/解码（1 writer + 3 reader）
- [x] 阶段 3: 版本 bump（4 常量 + 2 changelog）
- [x] 阶段 4: fixture regen + golden hex（zbc 6 + zpkg 4 + 2 golden hex + zpkg header pin + 3 expected.json + Rust 2 pinned + loader_tests helper）
- [x] 阶段 5: 测试 + 全 GREEN + 自举不动点（e2e 196/196 + cross-zpkg + stdlib + compiler 7/7 + vscode-syntax + cargo metadata 177/zbc_compat 3/lazy_loader 16/loader 48）
- [x] 阶段 6: 文档同步 + 归档

## 阶段 1: varint 原语
- [ ] 1.1 `ByteWriter.z42` 加 `WriteVarint(int)`（无符号 LEB128；负值防御）
- [ ] 1.2 `ZbcReader.z42` 的 `ZbcCursor` 加 `Varint()` + 越界保护
- [ ] 1.3 `ZpkgReader.z42` 的 `ZpkgCursor` 加 `Varint()` + 越界保护
- [ ] 1.4 `zbc_reader.rs` 的 `Cursor` 加 `read_varint()`（`bail!` 越界/溢出）

## 阶段 2: STRS 编码/解码
- [ ] 2.1 `ZbcWriter.BuildStrs` 重写为 segment-dict（段字典 first-seen + strTable 段序列）
- [ ] 2.2 `ZbcReader._readStrs` 改读 segment-dict + join('.') 重组
- [ ] 2.3 `ZpkgReader.Open` 的 STRS 分支改读 segment-dict（含 `dataStart` 计算移除）
- [ ] 2.4 Rust `read_strs` 改读 segment-dict + join('.') 重组
- [ ] 2.5 z42c 单测：`Std.A`/`Std.B`/`Std.A.C` 往返 + 段去重断言（zbc_tests.z42）

## 阶段 3: 版本 bump
- [ ] 3.1 `ZbcFormat.z42` `ZbcVersion.Minor` 20→21 + bump 注释
- [ ] 3.2 `ZpkgWriter.z42` `ZpkgWriterZ.Minor` 24→25 + bump 注释
- [ ] 3.3 `zbc_reader.rs` `ZBC_VERSION_MINOR` 20→21 + changelog 行
- [ ] 3.4 `zbc_reader.rs` `ZPKG_VERSION_MINOR` 24→25 + changelog 行（引用 zbc 1.21）

## 阶段 4: fixture regen + golden hex
- [ ] 4.1 `cargo build`（debug）+ `--release` 双 VM 重建（避免 regen 波用旧 minor VM）
- [ ] 4.2 `xtask build compiler && xtask build stdlib`（新 writer emit）
- [ ] 4.3 `xtask build test` → zbc-format 6 fixture 原地重生
- [ ] 4.4 手工 regen zpkg-format 4 fixture（packed-minimal/packed-multi-module/indexed-minimal/sym-only-sidecar）
- [ ] 4.5 `zbc_tests.z42` empty byte-identical hex 重截（`xxd -p empty/source.zbc`）
- [ ] 4.6 `zpkg_tests.z42` packed header pinned minor 0x18→0x19（+ 若断言含 STRS 字节则重截）

## 阶段 5: 测试 + 全 GREEN
- [ ] 5.1 `cargo test --test zbc_compat`（Rust 读 committed zbc 基线）
- [ ] 5.2 `cargo test lazy_loader`（Rust 读 committed zpkg 基线）
- [ ] 5.3 `xtask test`（全 stage：e2e + cross-zpkg + stdlib + compiler + vscode-syntax）
- [ ] 5.4 自举不动点 gen1==gen2 byte-identical（段序确定性）
- [ ] 5.5 spec 6 scenario 逐条覆盖确认

## 阶段 6: 文档 + 归档
- [ ] 6.1 `docs/design/runtime/zbc.md` STRS 布局 + Minor changelog 1.21
- [ ] 6.2 `docs/design/runtime/zpkg.md` Minor changelog 0.25
- [ ] 6.3 `BinaryFormat/README.md` STRS 段描述（若含）
- [ ] 6.4 归档 + ACTIVE.md 释放 compiler+runtime 锁（若 Change B 立即接续则转持有）
- [ ] 6.5 commit + push；盯 CI 两代 bump 路径（本次首次真实触发）

## 备注
- **本地两代自举 bump 路径首次真实跑通**（gen1 旧种子 → gen1-stdlib → gen2 0.25 → 新 VM 接管），
  自举不动点 gen2==gen3 逐字节 7/7。CI 两代 bump 路径待此 commit push 后观察。
- **实施期踩坑（重要，已根治 + 文档化）**：`ZpkgReader.Open` STRS segment-dict 解码循环里用了
  局部变量名 `name`，**与外层 META 包名 `name` 冲突**——z42 局部变量非块作用域 shadow → 内层
  复用/覆盖外层 → `z.Name` 被池里最后一个串污染 → `_isPrelude(z.Name)` 恒 false → prelude 包
  （z42.core）永不激活 → **所有 no-`using`/bare 名（`Assert`/prelude 等）解析回 null → 136 个
  无 using 的 e2e golden 全红**。自举不动点抓不到（z42c 源全用显式 using）。修：内层变量改名
  `poolStr`。规则沉淀 → `compiler-z42c.md` 受限写法。诊断关键：pristine 0.24 主线正常 + STRS 数据
  经独立 python 解码验证正确 + `using Std` 能解析而 bare 不能 → 锁定 z.Name 污染。
- Change B（`split-compile-sidecar`，用户 ①）在本 change 归档后接续，复用 compiler+runtime 锁。
