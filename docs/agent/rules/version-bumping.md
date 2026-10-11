---
paths:
  - "src/compiler/z42.package/src/**"
  - "src/runtime/src/metadata/**"
  - "docs/internals/src/formats/zbc.md"
  - "docs/internals/src/formats/zpkg.md"
  - "src/compiler/z42.package/tests/fixtures/zbc-format/**"
  - "src/compiler/z42.package/tests/fixtures/zpkg-format/**"
---

# `.zbc` / `.zpkg` minor version bump checklist

> z42 pre-1.0 **strict-pin** 政策：Rust reader 精确匹配 writer 的 major + minor，无兼容回退。
> 兼容性原则（"不为旧版本提供兼容"）见 [philosophy.md](philosophy.md#不为旧版本提供兼容)。
>
> 这份文件只回答一个问题：bump version 时**具体要同步改哪些文件**才能让 strict-pin 不变量 + golden 门通过。
> z42c（编译器）是 writer，z42vm（Rust）是 reader，两端版本常量必须同 commit 一起改。
>
> bump 之后**在本地怎么重建工具链、重生 fixture、跑门**（新格式本地建不动时的 overlay 配方）见
> [格式 bump 期的本地验证](../../internals/src/devinfra/format-bump.md)。

---

## 版本常量坐标（唯一真相表）

| 端 | 文件 | 常量 | 当前值 |
|----|------|------|--------|
| zbc writer（z42c） | `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` | 1 / 46 |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` | 1 / 46 |
| zpkg writer（z42c） | `src/compiler/z42.package/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` | 0 / 52 |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` | 0 / 52 |

> 🔒 **本表有防腐门**：`cargo test --test format_fixture_versions` 的
> `version_bumping_coordinate_table_matches_the_real_constants` **解析本表自己的四行**，
> 拿每行的「值」去和**该行指着的那个文件**里的常量对账 —— 路径写错 / 常量改名 / 值过期，
> 任一即红；行数不是 4 也红（防止悄悄删行）。同文件的
> `writer_and_reader_pin_the_same_format_versions` 另外钉住 writer↔reader 不许偏斜。
>
> ⚠️ 为什么需要这道门：这张表**是散文**：没有任何构建、测试或 strict-pin 校验会读它，而它偏偏是
> 承重的——过期的行会把下一个 bump 的人送到错的文件或错的起始号上。形态同步骤 4 / 9 的两道 fixture 门。

> ⚠️ **本页所有 `cargo test` 自检都不在 `xtask test` 里**（`xtask test` 的 stage 不含 `cargo test`，
> 那套只在 CI 的 `test-host` 四平台跑）。bump 后务必本地手跑一遍，别等 CI。

> reader 端（`zbc_reader/versions.rs`）每个常量旁有逐行 minor changelog 注释（日期 / PR / 格式变化）——
> bump 时在那里追加一行。writer 端常量旁也有同样的单行 bump 注释，保持格式一致。

---

## Bumping `.zbc` minor version

修改 `.zbc` wire format（新 opcode / 新 section / 已定义 section 字段语义变化）时，**单次 commit 必须同步以下 5 处**，否则 Rust reader strict-pin 校验、`zbc_compat` 字节基线、或 z42c golden hex 单测任一会 fail：

1. **`ZbcFormat.z42`**（`src/compiler/z42.package/src/BinaryFormat/`）— `ZbcVersion.Minor++`，常量旁注释本次 bump 内容（参考已有行格式）。若 bump 改了指令/section 布局，`ZbcInstr.z42`（编码）+ `ZbcReaderInstr.z42`（解码）或 `ZbcWriter.z42` 的对应 `Build*` / `_assemble` 逻辑同步。
2. **`zbc_reader/versions.rs`**（`src/runtime/src/metadata/`）— `ZBC_VERSION_MINOR` 同步到新值，**同时改钉值单测**
   `zbc_reader_tests.rs` 的 `zbc_version_constants_pinned` / `zpkg_version_constants_pinned`；并在常量上方
   changelog 注释块追加一行（日期 / PR / 字段变化）；reader 解码逻辑（`read_*_section`）同步新格式。
3. **`docs/internals/src/formats/zbc.md`** — "Minor changelog" 表加一行（minor / 日期 / 触发变更 / 引入内容）。
4. **regen zbc-format fixture** — 跑 `xtask build test`（前置 `build compiler`+`build stdlib` 已用新格式重建），原地覆写 `src/compiler/z42.package/tests/fixtures/zbc-format/*/source.zbc`（6 个 committed 字节基线：`empty` / `strp-func-minimal` / `multi-method` / `with-tidx` / `cross-import-token` / `with-frcs`）；`git diff` 应显示格式 delta，**必须连同 bump 一起提交**。

   > 🔒 **CI 有门（`refresh-format-fixtures`）**：`test-host` 的三条非 Windows 腿在
   > `test`（其 build wave 就地 regen）之后跑 `git diff --quiet -- src/compiler/z42.package/tests/fixtures/zbc-format`，**有差异即红**。
   > （该门在 `test-host`：三个架构都覆盖，失败经 `ci-ok`——main 唯一的必需检查——挡合并。）
   >
   > 为什么需要这道门：regen 在所有消费者
   > **之前**就地覆写，于是 `zbc_compat` 校验的永远是刚重生的字节、**从不是 committed 的那份** ——
   > 陈旧基线因此可以一直绿着，同时把每个人的工作树弄脏。形态同 `cargo fmt --check`。
5. **z42c golden hex 单测 —— `zbc_tests.z42` 里有 *三* 个逐字节断言，不是一个**
   （三个都要改）：

   | 测试 | 内嵌什么 |
   |---|---|
   | `test_zbc_empty_byte_identical` | `empty/source.zbc` 全量 hex（231B）|
   | `test_zbc_f5_with_dbug_byte_identical` | `int F(){return 5;}` 的全量 hex（含 DBUG 行表）|
   | `test_zbc_selfcheck_program_header` | 自检程序 header 前 10 字节 `5a4243 0001 <minor> 0200` |

   ⭐ **改之前先逐字节 diff，把「为什么只有这些字节变」写进注释** —— 那句推理才是 golden 的价值
   所在（例：1.46 那次三个用例都**一条 struct 指令都不发** ⇒ 只有 header 的 minor 那一个字节变）。
   只改数字、不记理由，下一个人无法判断 diff 是否合理。

   第一个从 regen 后的 fixture 重截：
   ```bash
   xxd -p src/compiler/z42.package/tests/fixtures/zbc-format/empty/source.zbc | tr -d '\n'
   ```
   验证：`xtask test compiler`（z42c zbc 单元须绿）。

提交前自检：

```bash
xtask build compiler && xtask build stdlib   # 用新格式重建 z42c + stdlib（fixture 须由新 writer emit）
xtask build test       # zbc-format 6 fixture 原地重生 + run-golden zbc 重生
cargo test --test zbc_compat    # Rust reader 读 committed zbc 字节基线
xtask test compiler    # z42c golden hex 单测
```

由于 strict-pin，minor bump 必然让所有现存 `.zbc` artifacts 失效；`xtask build test` 把 fixture + run-golden zbc 一并重生。这是预期行为，不需要兼容代码。

> 只修 reader / writer 的非格式 bug（不改 wire layout）— **不要** bump minor；strict-pin 仍通过。

> **下次 zbc bump 时顺带要删的 wire 残留**（`MkClos` 的 `u8` 尾字节等）登记在
> [zbc.md 的 `## Deferred` 节](../../internals/src/formats/zbc.md)，bump 时一并删掉。

---

## zpkg 联动规则（强耦合）

**zbc minor bump 必须同步 bump zpkg minor**（zpkg 内嵌 zbc，见 `docs/internals/src/formats/zpkg.md`）。在上述 5 步外加：

6. **`ZpkgWriter.z42`**（`src/compiler/z42.package/src/`）— `ZpkgWriterZ.Minor++`，注释更新内嵌 zbc 版本。
7. **`zbc_reader/versions.rs`** — `ZPKG_VERSION_MINOR` 同步；上方 zpkg changelog 注释块追加一行（指明耦合的 inner zbc minor）。
8. **`docs/internals/src/formats/zpkg.md`** — 更新页首「状态: ✅ 已实现（vX.YY）」与 `## 版本` 段的
   **当前配对**（`当前 0.NN ↔ 1.MM`，两处）。
   ⚠️ **本页没有 Minor changelog 表**。
   zpkg 的逐 minor 历史写在写端常量旁：`ZpkgWriter.z42` 的 `ZpkgWriterZ.Minor` 注释（步骤 6 已覆盖）。
9. **regen zpkg-format fixture** — 覆写 `src/compiler/z42.package/tests/fixtures/zpkg-format/*/source.zpkg`（4 个 committed 基线：`packed-minimal` / `packed-multi-module` / `indexed-minimal` / `sym-only-sidecar`）。
   每个 fixture 目录自带 **committed 构建配方 `<fixture>.z42.toml`**：
   `[project].pack` 决定 packed/indexed，是否带 `--release` 决定 strip/sidecar。
   完整重生命令见 [`src/compiler/z42.package/tests/fixtures/zpkg-format/README.md`](../../../src/compiler/z42.package/tests/fixtures/zpkg-format/README.md)「维护流程」；
   格式 bump 期的本地配方见[格式 bump 期的本地验证](../../internals/src/devinfra/format-bump.md)。

   > 🔒 **有防腐门**：`cargo test --test format_fixture_versions` 读 committed 字节、断言 header 版本
   > == 当前常量，**陈旧即红**（zbc 与 zpkg 两套一起覆盖）。

提交前自检扩展：

```bash
cargo test lazy_loader          # Rust reader 读 committed zpkg 字节基线
```

---

## Bumping `.zpkg` minor version（independent）

仅改 zpkg outer（不动 zbc）时（如新增 zpkg-only section / 已定义 section 字段语义）：只触步骤 6–9（zpkg writer / Rust 常量 / zpkg.md 当前配对 / zpkg fixture regen），跳过 zbc 步骤 1–5。

注意：实际工作中 zpkg-only 改动非常罕见（现有 minor bump 都耦合 zbc），但若发生，本节给出独立路径。

---

## bump 与 nightly 自愈

格式 bump 后：

- **从当前源码 bootstrap 的腿**（`build-and-test` / `compile-toolchain`）保持绿：`ci-bootstrap` 的
  **版本差 gate + 两代自举**（种子 minor ≠ 当前 writer minor 时用 nightly 自带的旧 VM 跑 gen1/gen2
  把种子推进到当前格式，再交 cargo 新 VM）自动处理，**无需手动传种子**。
- **纯 download-bootstrap 的腿**（`vm-jit` / `stdlib-jit`，用旧 nightly 的旧 VM）会 bump 当次一次性红，
  下一 run 下到新 nightly 自愈——它们不 feed `publish-nightly`，不阻塞。
- **`bench-regression` 不在此列**：它在 bump PR 上的红不是「旧 nightly」问题，而是 A/B 的 base 侧
  结构上不可测（base stdlib 被 PR 的 z42.package 写成 PR 格式、base VM 读不了）——检测到格式代差即
  **跳过 A/B 并打 warning**，不亮红。原理见
  [性能基准与回归门禁](../../internals/src/devinfra/benchmarking.md) §10。

**手动发布 nightly（escape hatch）**：若自愈不及时（或要在不推 commit 的情况下刷新 nightly），
手动触发 CI 的 `workflow_dispatch`：`gh workflow run CI --ref main`；vm-jit / bench 即使红也不挡发布。

> **硬约束**：任何 feed `publish-nightly` 的 job 必须从**当前源码** bootstrap（不许走 download-nightly
> composite），否则 publish 路径变成依赖旧 nightly，死锁复活。

> 机制全文（`publish-nightly` 的 `needs` 为什么故意不含 jit 腿、job 表、逃生口）见
> [CI 拓扑](../../internals/src/devinfra/ci.md)；格式 bump 不要踩在会让旧 nightly 读不了当前源码的时机，
> 见 [bootstrap-seed.md](bootstrap-seed.md)。

---

## 编译器语义指纹（非格式失效次元）

> 触发条件：改了 **z42c 的 codegen / 优化 pass / typecheck / lowering 行为**，但 **zbc/zpkg
> 格式 Minor 没有 bump**（即同一份源码、同样的 wire 格式，编出的 `.zbc` 字节却会变）。

| 情形 | 动作 |
|------|------|
| 改 codegen / 优化 pass / typecheck / lowering / **发出的诊断**，**且不 bump zbc/zpkg 格式** | 在 `CompilerFingerprint.Entries` **末尾追加一行本次变更的 slug** |
| bump 了 zbc/zpkg 格式 Minor | **不必**追加——格式 Minor 变化已让所有旧 `.meta` 失效（追加了也无害）|
| 只修 reader/writer 非格式 bug（不改 wire、不改编出的字节、不改诊断） | 不追加 |

**为什么需要这个失效次元**（增量缓存只 pin 源 hash + 格式 Minor，测不出"编译器语义变了但格式没变"）、
**CI 守门**（bench-pr 的 Compiler fingerprint guard，以及它对「诊断变了、发码不变」那一档的结构性失明）、
**本地复现**与坐标（`CompilerFingerprint.z42` 的 `Entries`，只许末尾追加）全部见
[编译器语义指纹（CompilerFingerprint）](../../internals/src/compiler/compiler-fingerprint.md)——本页不复列。
