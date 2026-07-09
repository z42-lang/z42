# Proposal: STRS 字符串池重编码为 segment-dict

## Why

发布的 `.zbc` / `.zpkg` 中 STRS（字符串池）是体积第二大的段（聚合 22 个 stdlib 包占
23.2%）。实测其编码有两处冗余：

1. **index 表存了可推导的 offset**：`count×(u32 offset, u32 len)`，而 offset 恒为
   前缀和（`offset[i] = Σ len[0..i]`）——纯冗余。z42.core 单包 index 表 12212B，其中
   offset 占一半。
2. **FQ 名前缀重复**：`Std.`、`Std.String.`、`Std.Collections.Dictionary.` 等
   namespace 段在 1526 个串里反复出现；实测唯一段仅 1006 个（去重率 34%）。

把两者一次解决：STRS 改为 **segment-dict** 编码——按 `.` 切分所有池串，唯一段去重存
一次，每个串表示为「段索引序列」。实测 z42.core STRS **40420B → 22475B（−44.4%）**，
是单纯 varint 方案（−26.4%）的近两倍，且同时消掉 offset 冗余与前缀重复。STRS 占聚合
23% → 整包约 **−10%**，`.zbc` / `.zpkg`、SDK 与部署产物全部受益。

不做的话：每个包持续携带 ~40% 可压缩的字符串池冗余。

## What Changes

- STRS 段 wire 布局从 `count + (u32 offset,u32 len)×count + utf8-blob` 改为
  `segCount + 段字典(varint len + utf8)×segCount + strCount + (varint segN + varint segIdx×segN)×strCount`。
- 串索引（其它段引用 STRS 的方式）**不变**：仍是 0-based 池位置；segment-dict 只改
  STRS 段内部编码，`strCount` 顺序 == 旧池顺序，故 SIGS/MODS/TSIG/NSPC/EXPT/DEPS 等
  所有 `pool.Idx()` 引用零改动。
- 新增无符号 varint（LEB128）原语：writer `ByteWriter.WriteVarint`，reader
  `ZbcCursor`/`ZpkgCursor`（z42）+ `Cursor`（Rust）的 `ReadVarint`/`read_varint`。
- `.zbc` minor 1.20→1.21，`.zpkg` minor 0.24→0.25（强耦合，同 commit bump）。
- 重生 zbc-format(6) + zpkg-format(4) fixture + z42c golden hex（empty.zbc 内嵌串）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.ir/src/BinaryFormat/ByteWriter.z42` | MODIFY | 加 `WriteVarint(int)`（LEB128 无符号） |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | `BuildStrs` 重写为 segment-dict |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 1.20→1.21 + bump 注释 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | `_readStrs` 改读 segment-dict；`ZbcCursor.Varint()` |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 0.24→0.25 + bump 注释（STRS 经 `BuildStrs` 自动跟随，无逻辑改动） |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | `Open` 的 STRS 解析改 segment-dict；`ZpkgCursor.Varint()` |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | `read_strs` 改读 segment-dict + `Cursor::read_varint`；`ZBC_VERSION_MINOR` 20→21、`ZPKG_VERSION_MINOR` 24→25 + changelog 注释 |
| `docs/design/runtime/zbc.md` | MODIFY | STRS 段布局描述 + Minor changelog 加 1.21 行 |
| `docs/design/runtime/zpkg.md` | MODIFY | Minor changelog 加 0.25 行（引用同次 zbc bump） |
| `src/tests/zbc-format/empty/source.zbc` | MODIFY | regen（segment-dict 布局） |
| `src/tests/zbc-format/strp-func-minimal/source.zbc` | MODIFY | regen |
| `src/tests/zbc-format/multi-method/source.zbc` | MODIFY | regen |
| `src/tests/zbc-format/with-tidx/source.zbc` | MODIFY | regen |
| `src/tests/zbc-format/cross-import-token/source.zbc` | MODIFY | regen |
| `src/tests/zbc-format/with-frcs/source.zbc` | MODIFY | regen |
| `src/tests/zpkg-format/packed-minimal/source.zpkg` | MODIFY | regen |
| `src/tests/zpkg-format/packed-multi-module/source.zpkg` | MODIFY | regen |
| `src/tests/zpkg-format/indexed-minimal/source.zpkg` | MODIFY | regen |
| `src/tests/zpkg-format/sym-only-sidecar/source.zpkg` | MODIFY | regen（sidecar STRS 亦经 BuildStrs） |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | `test_zbc_empty_byte_identical` 内嵌 hex 重截 |
| `src/compiler/z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | packed header pinned minor 0x18→0x19（若断言含 STRS 字节则同步） |
| `src/compiler/z42c.ir/src/BinaryFormat/README.md` | MODIFY | STRS 布局段更新（若有） |

**只读引用**：

- `src/compiler/z42c.ir/src/BinaryFormat/ZbcStringPool.z42` — 段字典复用其 intern 语义
- `src/libraries/z42.io.binary/src/BinaryWriter.z42:197` — 参照 LEB128 写法
- `.claude/rules/version-bumping.md` — bump checklist
- `.claude/rules/bootstrap-seed.md` — 两代自举吸收本次格式 bump

## Out of Scope

- 编译-sidecar（`.zsig`）拆分 TSIG/IMPL/EXPT —— 独立后续 change B
  `split-compile-sidecar`（用户 ① 项）。
- MODS / SIGS / TSIG 等其它段的编码优化。
- 段字典排序压缩（front-coding）——segment-dict 已捕获主要收益，进一步压缩边际低。

## Open Questions

- 无（segment-dict 布局已由实测锚定；串索引不变保证消费端零改动）。
