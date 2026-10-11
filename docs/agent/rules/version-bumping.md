# `.zbc` / `.zpkg` 格式 bump 纪律

> z42 pre-1.0 **strict-pin**：Rust reader 精确匹配 writer 的 major + minor，无兼容回退。
> 兼容性原则（"不为旧版本提供兼容"）见 [philosophy.md](philosophy.md#不为旧版本提供兼容)。
>
> **「改哪些文件、怎么在本地重建与重生 fixture」的操作清单（唯一 SoT）在书里**：
> [格式 bump：清单与本地验证](../../internals/src/devinfra/format-bump.md)。

## 纪律（必须遵守）

1. **writer 与 reader 的版本常量必须同 commit 一起改**——z42c 是 writer、z42vm（Rust）是 reader，
   strict-pin 不匹配直接拒读，没有兼容回退、没有 fallback。
2. **改了 wire 格式就必须 bump minor + regen committed 基线**（fixture + golden hex）并同步两端 changelog；
   bump 必然让所有现存 `.zbc` artifacts 失效，这是预期行为，不需要兼容代码。
3. **只修 reader / writer 的非格式 bug（不改 wire layout）→ 不要 bump**；strict-pin 仍通过。
4. **改完必须跑清单里的自检**。注意它们**不在 `xtask test` 里**（`xtask test` 的 stage 不含 `cargo test`，
   那套只在 CI 的 `test-host` 四平台跑），别等 CI 才发现。

## 版本常量坐标（唯一真相表）

| 端 | 文件 | 常量 | 当前值 |
|----|------|------|--------|
| zbc writer（z42c） | `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` | 1 / 46 |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` | 1 / 46 |
| zpkg writer（z42c） | `src/compiler/z42.package/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` | 0 / 52 |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` | 0 / 52 |

> 🔒 **本表有防腐门**：`cargo test --test format_fixture_versions` 的
> `version_bumping_coordinate_table_matches_the_real_constants` **解析本表自己的四行**，
> 拿每行的「值」去和**该行指着的那个文件**里的常量对账 —— 路径写错 / 常量改名 / 值过期，任一即红；
> 行数不是 4 也红（防止悄悄删行）。同文件的 `writer_and_reader_pin_the_same_format_versions`
> 另外钉住 writer↔reader 不许偏斜。
>
> ⚠️ 这张表**是散文**：没有任何构建会读它，而它偏偏是承重的——过期的行会把下一个 bump 的人送到
> 错的文件或错的起始号上，所以给它配了上面那道门。**本表留在本文件**（测试按此路径解析），
> 变动时两处一起改。

## 编译器语义指纹（非格式失效次元）

> 触发条件：改了 **z42c 的 codegen / 优化 pass / typecheck / lowering 行为**（含**发出的诊断**），
> 但 **zbc/zpkg 格式 Minor 没有 bump**（同一份源码、同样的 wire 格式，编出的 `.zbc` 字节却会变）。

| 情形 | 动作 |
|------|------|
| 改 codegen / 优化 pass / typecheck / lowering / **发出的诊断**，**且不 bump zbc/zpkg 格式** | 在 `CompilerFingerprint.Entries` **末尾追加一行本次变更的 slug** |
| bump 了 zbc/zpkg 格式 Minor | **不必**追加——格式 Minor 变化已让所有旧 `.meta` 失效（追加了也无害）|
| 只修 reader/writer 非格式 bug（不改 wire、不改编出的字节、不改诊断） | 不追加 |

机制（为什么需要这个失效次元）、CI 守门与本地复现见
[编译器语义指纹](../../internals/src/compiler/compiler-fingerprint.md)——本文件不复列。

## 与 CI 的关系

bump 后 `vm-jit` / `bench` 这类**纯 download-bootstrap** 的 job 会一次性红、下一 run 自愈；从当前源码
bootstrap 的腿由 `ci-bootstrap` 的版本差 gate + 两代自举自动处理，**无需手动传种子**。机制见
[CI 拓扑](../../internals/src/devinfra/ci.md)；`bench-regression` 在格式代差 PR 上跳过 A/B 见
[性能基准与回归门禁](../../internals/src/devinfra/benchmarking.md)。
