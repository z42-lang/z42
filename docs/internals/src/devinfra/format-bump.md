# 格式 bump：清单与本地验证

> 代码：`src/compiler/z42.package/src/BinaryFormat/`、`src/runtime/src/metadata/`
> 相关：[zbc 格式](../formats/zbc.md) · [zpkg 格式](../formats/zpkg.md) · [CI 拓扑](ci.md) · [测试怎么跑](testing.md)
>
> §一 是**改哪些文件**的同步清单（唯一 SoT）；§二 是 bump 后**在本地怎么重建工具链、重生 fixture、跑门**
> （新格式本地建不动时的 overlay 配方）。

zbc / zpkg minor bump 后，本地有一个死结：**`cargo build` 出来的 z42vm 是新格式 reader，而本地唯一的种子
（`.z42/` 下的 nightly、或 warm `artifacts/`）还是旧格式 writer**——新 VM 读旧种子 zpkg 直接撞墙：

```
zpkg minor <旧> not supported (writer is at <新>)
```

把种子推进到新格式本需**两代自举**（旧 VM 跑 gen1/gen2），而这条链在本地（尤其 macOS）不可用。
**解法：让 CI 先把新格式工具链建出来，下载回本地 overlay 成种子**——种子与 cargo VM 同为新格式之后，
warm 建 / 测 / fixture 重生全部走通，两代自举彻底不需要。

> zpkg 是可移植字节码——linux 建出的 `z42c.driver.zpkg` 也能在 macOS 的 cargo VM 上跑（CI 的 macOS /
> Windows job 一直这么用），所以只需 linux 一份 artifact。

## 一、bump 清单（唯一 SoT）

改 `.zbc` / `.zpkg` wire 格式（新 opcode / 新 section / 已定义 section 字段语义变化）时，**单次 commit 必须同步以下各处**，
否则 Rust reader 的 strict-pin 校验、`zbc_compat` 字节基线、或 z42c golden hex 单测任一会 fail。

### 版本常量坐标（唯一真相表）

z42 pre-1.0 是 **strict-pin**：Rust reader 精确匹配 writer 的 major + minor，没有兼容回退。四个常量在哪如下；
**当前版本号以代码为准，文档不抄数值**——抄了就要每次 bump 都跟着改，而且迟早会漏。

| 端 | 文件 | 常量 |
|----|------|------|
| zbc writer（z42c） | `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` |
| zpkg writer（z42c） | `src/compiler/z42.package/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` |

> 🔒 **本表有防腐门**：`cargo test --test format_fixture_versions` 的
> `version_bumping_coordinate_table_matches_the_real_constants` **解析本表自己的四行**，到每行指着的文件里找该行
> 写的常量——路径写错 / 常量改名，任一即红；行数不是 4 也红。同文件的
> `writer_and_reader_pin_the_same_format_versions` 钉住 writer↔reader 的数值不许偏斜。这张表是散文，没有构建会读它，
> 而它偏偏是承重的（过期的行会把下一个 bump 的人送到错的文件上），所以配了这道门。
> 门按本页路径解析，**挪表时同一提交改测试**。

### Bumping `.zbc` minor version —— 5 步

1. **`src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42`** — `ZbcVersion.Minor++`，常量旁注释本次 bump 内容
   （参考已有行格式）。若 bump 改了指令 / section 布局，`ZbcInstr.z42`（编码）+ `ZbcReaderInstr.z42`（解码）或
   `ZbcWriter.z42` 的对应 `Build*` / `_assemble` 逻辑同步。
2. **`src/runtime/src/metadata/zbc_reader/versions.rs`** — `ZBC_VERSION_MINOR` 同步，**同时改钉值单测**
   `zbc_reader_tests.rs` 的 `zbc_version_constants_pinned` / `zpkg_version_constants_pinned`；并在常量上方 changelog
   注释块追加一行（日期 / PR / 字段变化）；reader 解码逻辑（`read_*_section`）同步新格式。
3. **[zbc.md](../formats/zbc.md)** — 格式描述（指令表 / section 布局）变了就同步对应章节；页内不记版本历史。
4. **regen zbc-format fixture** — 跑 `xtask build test`（前置 `build compiler` + `build stdlib` 已用新格式重建），
   原地覆写 `src/compiler/z42.package/tests/fixtures/zbc-format/*/source.zbc`（该目录下全部 committed 字节基线）；`git diff` 应显示格式
   delta，**必须连同 bump 一起提交**。

   > 🔒 **CI 有门（`refresh-format-fixtures`）**：`test-host` 的三条非 Windows 腿在 `test`（其 build wave 就地 regen）
   > 之后跑 `git diff --quiet -- src/compiler/z42.package/tests/fixtures/zbc-format`，**有差异即红**。
   > 为什么需要这道门：regen 在所有消费者**之前**就地覆写，于是 `zbc_compat` 校验的永远是刚重生的字节、
   > **从不是 committed 的那份** —— 陈旧基线因此可以一直绿着，同时把每个人的工作树弄脏。
5. **z42c golden hex 单测 —— `zbc_tests.z42` 里有 *三* 个逐字节断言，不是一个**（三个都要改）：

   | 测试 | 内嵌什么 |
   |---|---|
   | `test_zbc_empty_byte_identical` | `empty/source.zbc` 全量 hex |
   | `test_zbc_f5_with_dbug_byte_identical` | `int F(){return 5;}` 的全量 hex（含 DBUG 行表）|
   | `test_zbc_selfcheck_program_header` | 自检程序 header 前 10 字节 `5a4243 0001 <minor> 0200` |

   ⭐ **改之前先逐字节 diff，把「为什么只有这些字节变」写进注释** —— 那句推理才是 golden 的价值所在
   （例：bump 只改了 struct 指令的编码，而三个用例都**一条 struct 指令都不发** ⇒ 只有 header 的 minor 那一个字节变）。
   第一个从 regen 后的 fixture 重截：
   ```bash
   xxd -p src/compiler/z42.package/tests/fixtures/zbc-format/empty/source.zbc | tr -d '\n'
   ```
   验证：`xtask test compiler`（z42c zbc 单元须绿）。

### zpkg 联动（强耦合）—— 4 步

**zbc minor bump 必须同步 bump zpkg minor**（zpkg 内嵌 zbc，见 [zpkg.md](../formats/zpkg.md)）。在上述 5 步外加：

6. **`src/compiler/z42.package/src/ZpkgWriter.z42`** — `ZpkgWriterZ.Minor++`，注释更新内嵌 zbc 版本。
7. **`zbc_reader/versions.rs`** — `ZPKG_VERSION_MINOR` 同步；上方 zpkg changelog 注释块追加一行（指明耦合的 inner zbc minor）。
8. **[zpkg.md](../formats/zpkg.md)** — 页内不写当前版本号；只有 outer 布局变化时才改对应章节。
   zpkg 的逐 minor 记录写在写端常量旁（步骤 6 已覆盖）。
9. **regen zpkg-format fixture** — 覆写 `src/compiler/z42.package/tests/fixtures/zpkg-format/*/source.zpkg`
   （该目录下全部 committed 基线）。
   每个 fixture 目录自带 **committed 构建配方 `<fixture>.z42.toml`**：`[project].pack` 决定 packed / indexed，
   是否带 `--release` 决定 strip / sidecar。完整重生命令见
   [zpkg-format fixture README](../../../../src/compiler/z42.package/tests/fixtures/zpkg-format/README.md)「维护流程」。

   > 🔒 **有防腐门**：`cargo test --test format_fixture_versions` 读 committed 字节、断言 header 版本 == 当前常量，
   > **陈旧即红**（zbc 与 zpkg 两套一起覆盖）。

**仅改 zpkg outer（不动 zbc）时**：只触步骤 6–9，跳过 1–5。实际工作中很罕见（现有 minor bump 都耦合 zbc）。

### 提交前自检

```bash
xtask build compiler && xtask build stdlib   # 用新格式重建 z42c + stdlib（fixture 须由新 writer emit）
xtask build test                             # zbc-format 6 fixture 原地重生 + run-golden zbc 重生
cargo test --test zbc_compat                 # Rust reader 读 committed zbc 字节基线
cargo test lazy_loader                       # Rust reader 读 committed zpkg 字节基线
xtask test compiler                          # z42c golden hex 单测
```

> 只修 reader / writer 的非格式 bug（不改 wire layout）— **不要** bump minor；strict-pin 仍通过。
> **下次 zbc bump 时顺带要删的 wire 残留**（`MkClos` 的 `u8` 尾字节等）登记在
> [zbc.md 的 `## Deferred` 节](../formats/zbc.md)，bump 时一并删掉。

---

## 二、本地重建与 fixture 重生

前置：bump 清单（§一）各步已改完源码与版本常量。

### 1. 先推一个 PR，拿工具链 artifact

首轮 CI 里 `compile-toolchain` 应绿——它从当前 PR 源码建出新格式的 z42c + 全 stdlib，上传为
`toolchain-ubuntu-latest`。`test-host` 这一轮预期**红在 committed fixture**（还是旧格式、尚未重生），
属正常；这轮只为拿 artifact。

### 2. 下载 + overlay（保留本树的 `runtime/z42vm`）

```bash
RUN=<compile-toolchain 所在 run-id>          # gh run list --branch <your-branch>
gh run download $RUN -n toolchain-ubuntu-latest -D /tmp/tc
rm -rf artifacts/build/compiler artifacts/build/libraries
cp -R /tmp/tc/artifacts/build/compiler   artifacts/build/
cp -R /tmp/tc/artifacts/build/libraries  artifacts/build/
cp    /tmp/tc/artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zpkg
```

🔴 **`xtask.zpkg` 很可能比 `libraries/` 旧一代——拷完必须重建它。** artifact 里的 `xtask.zpkg` 是流水线
**早期**用**种子 stdlib** 建的，同一个 artifact 里的 `libraries/` 是**末期**的新格式产物，两者差一代。
症状是一个离现场很远的缺符号：

```
Error: uncaught exception: Std.MissingSymbolException:
  undefined function `Z42.Project.ManifestLoader.LoadWorkspace$1$string`
  at Z42Xtask._wsBuildRoot(string)        ← 判据：栈顶在 `Z42Xtask.*`
  at Z42Xtask._ensureSeed(string)            ⇒ 是 xtask 二进制自己缺符号，
  at Z42Xtask._buildCompiler()               **不是**被测代码的问题
```

修法是用刚 overlay 进来的新 z42c 直接编 xtask 源：

```bash
rm -f artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zsym    # 不删则 publish 不重编
Z42_LIBS="$PWD/artifacts/intermediate/libraries/flat/release" \
  artifacts/build/runtime/release/z42vm \
  artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg \
  -- build scripts/xtask.z42.toml --release
```

⚠️ **不要**手工把某个 `.zpkg` 拷进 `Z42_LIBS` 去「修」依赖——那会弄坏依赖解析，报
「A skipped package is invisible to dependency resolution」外加一片**假的** `undefined: <Type>` /
`undefined function`，离真因更远。

### 3. 让 xtask 用本树的新格式 cargo VM

launcher 默认回落 `.z42/bin/z42vm`（旧种子），不设这个变量会报 `minor <新> not supported (writer is at <旧>)`：

```bash
export Z42_PORTABLE_VM="$PWD/artifacts/build/runtime/release/z42vm"
```

### 4. 在新格式下重建、重生、跑门

```bash
xtask build compiler && xtask build stdlib   # warm，同格式
xtask build test                             # 原地重生全部 zbc-format fixture
cargo test --lib                             # committed fixture 现应全过
xtask test                                   # 完整 GREEN，含自举不动点 gen1==gen2
```

**zpkg-format fixture 要手工重生**（`xtask build test` 不含 zpkg）：

```bash
VM=$PWD/artifacts/build/runtime/release/z42vm
Z42C=$(find /tmp/tc -name z42c.driver.zpkg | head -1)
LIBS=$PWD/artifacts/intermediate/libraries/flat/release
# 临时工程：name 用 demo.minimal / demo.multi / demo.indexed（按 fixture 目录名对应），kind=lib
#   packed → --release；indexed → 无 --release（另产散装 source.zbc）
Z42_LIBS="$LIBS" "$VM" "$Z42C" -- build <temp>/demo.minimal.z42.toml --release
cp <temp>/dist/demo.minimal.zpkg src/compiler/z42.package/tests/fixtures/zpkg-format/packed-minimal/source.zpkg
# indexed 另拷 dist/source.zbc → indexed-minimal/source.zbc（散装 + FILE 段 hash 自动同步）
```

`sym-only-sidecar` 没有 Rust 字节读测试 → 保持旧格式不动。各 fixture 目录自带 committed 构建配方
（`[project].pack` 决定 packed / indexed，是否带 `--release` 决定 strip / sidecar），完整重生命令见
[zpkg-format fixture README](../../../../src/compiler/z42.package/tests/fixtures/zpkg-format/README.md)。

### 5. 收尾

重生的 fixture 与收尾 commit 一起 push，PR 转全绿后合并。

## 为什么不用「本地两代自举」

两代自举由 `ci-bootstrap` 的版本差 gate 驱动（种子 minor ≠ 当前 writer minor 时用 nightly 自带的旧 VM
跑 gen1/gen2），是 CI 的常规路径、不需要人工传种子；它在本地的复现成本高于本页 overlay 一条路，
详见 [CI 拓扑](ci.md) 与 [编译器自举](../compiler/self-hosting.md)。
