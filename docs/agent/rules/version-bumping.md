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

---

## 版本常量坐标（唯一真相表）

| 端 | 文件 | 常量 | 当前值 |
|----|------|------|--------|
| zbc writer（z42c） | `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` | 1 / 46 |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` | 1 / 46 |
| zpkg writer（z42c） | `src/compiler/z42.package/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` | 0 / 51 |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader/versions.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` | 0 / 51 |

> 🔒 **本表有防腐门**：`cargo test --test format_fixture_versions` 的
> `version_bumping_coordinate_table_matches_the_real_constants` **解析本表自己的四行**，
> 拿每行的「值」去和**该行指着的那个文件**里的常量对账 —— 路径写错 / 常量改名 / 值过期，
> 任一即红；行数不是 4 也红（防止悄悄删行）。同文件的
> `writer_and_reader_pin_the_same_format_versions` 另外钉住 writer↔reader 不许偏斜。
>
> ⚠️ 为什么需要这道门：这张表**是散文**：没有任何构建、
> 测试或 strict-pin 校验会读它，而它偏偏是承重的——过期的行会把下一个 bump 的人送到
> 错的文件或错的起始号上。形态同步骤 4 / 9 的两道 fixture 门。
>
> ⚠️ 这两个测试只在 `cargo test --test format_fixture_versions` 里跑，**`xtask test` 不含**
> （它的 15 个 stage 既不含 `cargo test --lib` 也不含各 `--test`；那套只在 CI 的 `test-host`
> 四平台跑）。bump 后务必本地手跑一次，别等 CI。

> reader 端（`zbc_reader/versions.rs`）每个常量旁有逐行 minor changelog 注释（日期 / spec / 格式变化）——bump 时在那里追加一行。
> writer 端常量旁也有同样的单行 bump 注释，保持格式一致。

---

## Bumping `.zbc` minor version

修改 `.zbc` wire format（新 opcode / 新 section / 已定义 section 字段语义变化）时，**单次 commit 必须同步以下 5 处**，否则 Rust reader strict-pin 校验、`zbc_compat` 字节基线、或 z42c golden hex 单测任一会 fail：

1. **`ZbcFormat.z42`**（`src/compiler/z42.package/src/BinaryFormat/`）— `ZbcVersion.Minor++`，常量旁注释本次 bump 内容（参考已有行格式）。若 bump 改了指令/section 布局，`ZbcInstr.z42`（编码）+ `ZbcReaderInstr.z42`（解码）或 `ZbcWriter.z42` 的对应 `Build*` / `_assemble` 逻辑同步。
2. **`zbc_reader/versions.rs`**（`src/runtime/src/metadata/`）— `ZBC_VERSION_MINOR` 同步到新值（**同时改钉值单测**
   `zbc_reader_tests.rs` 的 `zbc_version_constants_pinned` / `zpkg_version_constants_pinned`——它们只在
   `cargo test --lib` 里跑，`xtask test` 不包含）；并在常量上方 changelog 注释块追加一行（日期 / spec / 字段变化）；reader 解码逻辑（`read_*_section`）同步新格式。
3. **`docs/internals/src/formats/zbc.md`** — "Minor changelog" 表加一行（minor / 日期 / 触发 spec / 引入内容）。
4. **regen zbc-format fixture** — 跑 `xtask build test`（前置 `build compiler`+`build stdlib` 已用新格式重建），原地覆写 `src/compiler/z42.package/tests/fixtures/zbc-format/*/source.zbc`（6 个 committed 字节基线：`empty` / `strp-func-minimal` / `multi-method` / `with-tidx` / `cross-import-token` / `with-frcs`）；`git diff` 应显示格式 delta，**必须连同 bump 一起提交**。

   > 🔒 **CI 有门（`refresh-format-fixtures`）**：`test-host` 的三条非 Windows 腿在
   > `test`（其 build wave 就地 regen）之后跑 `git diff --quiet -- src/compiler/z42.package/tests/fixtures/zbc-format`，**有差异即红**。
   > （该门在 `test-host`：三个架构都覆盖，且 `test-host` 是 required check。）
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
   完整重生命令见 [`src/compiler/z42.package/tests/fixtures/zpkg-format/README.md`](../../../src/compiler/z42.package/tests/fixtures/zpkg-format/README.md)「维护流程」。

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

## 本地全量验证 / fixture 重生的配方（格式 bump 专用）

> 引入新格式后，本地两代自举在 macOS 撞环境墙；解法是让 CI 先把新格式工具链建出来、
> 下载回本地当种子，即可在本地跑通完整 GREEN + fixture 重生，无需两代自举。

### 为什么本地直接建不动

minor bump 后，本地 `cargo build` 出的 z42vm 是**新格式**（reader 钉新 minor），但本地唯一的 z42c/stdlib
种子（`.z42/` 下载的 nightly、或 warm `artifacts/`）还是**旧格式**。warm 建 `xtask build compiler/stdlib`
→ 新 VM 读旧种子 zpkg → `zpkg minor <旧> not supported (writer is at <新>)` 直接墙掉。要把种子推进到新格式
本需**两代自举**（旧 VM 跑 gen1/gen2），而本地两代自举在 macOS 有独立的环境墙（见
`bootstrap-seed.md`）。→ 死结。

### 解法：下载 CI 建好的新格式工具链当本地种子

CI 的 `compile-toolchain` job（两代自举已根治）从**当前 PR 源码**建出新格式的 z42c + 全 stdlib，并
`upload-artifact` 为 `toolchain-ubuntu-latest`（只有 linux 一份——zpkg 与宿主无关，所有 OS 都用它）。把它下回本地
overlay 成种子，**种子与 cargo VM 就同为新格式** → warm 建/测/regen 全通，两代自举彻底不需要。

> zpkg 是可移植字节码——linux 建的 z42c.driver.zpkg 也能在 macOS cargo VM 上跑（CI 的 macOS / Windows job 一直这么用）。

**步骤**（承接上面「Bumping」各步已改完源码 + 版本常量）：

1. **先推一个 PR**。首轮 CI：`compile-toolchain` 应绿（新格式工具链建成 + 上传）；`test-host` 预期**红在
   committed fixture**（旧格式，还没重生）——正常，用这轮只为拿工具链 artifact。
2. **下载 + overlay**（保留你自己的 `runtime/z42vm`）：
   ```bash
   RUN=<compile-toolchain 所在 run-id>          # gh run list --branch <your-branch>
   gh run download $RUN -n toolchain-ubuntu-latest -D /tmp/tc
   rm -rf artifacts/build/compiler artifacts/build/libraries
   cp -R /tmp/tc/artifacts/build/compiler   artifacts/build/
   cp -R /tmp/tc/artifacts/build/libraries  artifacts/build/
   cp    /tmp/tc/artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zpkg
   ```

   🔴 **`xtask.zpkg` 很可能比 libraries 旧一代 —— 拷完必须重建它**。
   artifact 里的 `xtask.zpkg` 是流水线**早期**用**种子 stdlib** 建的，而同一个 artifact 里的
   `libraries/` 是**末期**的新格式产物 ⇒ 两者差一代。症状是一个**离现场很远**的缺符号：

   ```
   Error: uncaught exception: Std.MissingSymbolException:
     undefined function `Z42.Project.ManifestLoader.LoadWorkspace$1$string`
     at Z42Xtask._wsBuildRoot(string)        ← ⭐ 判据：栈顶在 `Z42Xtask.*`
     at Z42Xtask._ensureSeed(string)            ⇒ 是 xtask 二进制自己缺符号，
     at Z42Xtask._buildCompiler()               **不是**被测代码的问题
   ```

   修法（用刚 overlay 进来的新 z42c 直接编 xtask 源）：

   ```bash
   rm -f artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zsym    # 不删则 publish 不重编
   Z42_LIBS="$PWD/artifacts/intermediate/libraries/flat/release" \
     artifacts/build/runtime/release/z42vm \
     artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg \
     -- build scripts/xtask.z42.toml --release
   ```

   ⚠️ **不要**手工把某个 `.zpkg` 拷进 `Z42_LIBS` 去「修」依赖 —— 会弄坏依赖解析，报
   「A skipped package is invisible to dependency resolution」+ 一片**假的**
   `undefined: <Type>` / `undefined function`，离真因更远。
3. **强制 xtask 用你的新格式 cargo VM**（launcher 默认回落 `.z42/bin/z42vm` 旧种子 → 会报
   `minor <新> not supported (writer is at <旧>)`）：
   ```bash
   export Z42_PORTABLE_VM="$PWD/artifacts/build/runtime/release/z42vm"
   ```
4. 现在一切在新格式下跑通（无两代自举）：
   ```bash
   xtask build compiler && xtask build stdlib   # warm，同格式
   xtask build test                             # 原地重生 6 个 zbc-format fixture
   cargo test --lib                             # committed fixture 现应全过
   xtask test                                   # 完整 GREEN，含自举不动点 gen1==gen2
   ```
   **zpkg-format fixture（手工，步骤 9）**：`xtask build test` 不含 zpkg。逐个：
   ```bash
   VM=$PWD/artifacts/build/runtime/release/z42vm
   Z42C=$(find /tmp/tc -name z42c.driver.zpkg | head -1)
   LIBS=$PWD/artifacts/intermediate/libraries/flat/release
   # 临时工程：name 用 demo.minimal / demo.multi / demo.indexed，kind=lib
   #   （按 fixture 目录名对应）
   #   packed → --release；indexed → 无 --release（另产散装 source.zbc）
   Z42_LIBS="$LIBS" "$VM" "$Z42C" -- build <temp>/demo.minimal.z42.toml --release
   cp <temp>/dist/demo.minimal.zpkg src/compiler/z42.package/tests/fixtures/zpkg-format/packed-minimal/source.zpkg
   # indexed 另拷 dist/source.zbc → indexed-minimal/source.zbc（散装 + FILE 段 hash 自动同步）
   ```
   `sym-only-sidecar` 无 Rust 字节读测试 → 保持旧格式不动。
5. **重生的 fixture + 收尾 commit** 一起 push；PR 转全绿后合并。

---

## bump 与 xtask↔nightly bootstrap 循环

> **格式-bump 的 bootstrap 死结由 ci-bootstrap 处理**：ci-bootstrap
> 加了**版本差 gate + 两代自举**——种子 minor ≠ 当前 writer minor 时,用 nightly SDK 自带的
> **旧 VM**(bin/z42vm)跑 gen1/gen2 把种子推进到当前格式,再交 cargo 新 VM。所以 **zpkg/zbc
> minor bump 后 build-and-test / compile-toolchain 等**从当前源码 bootstrap 的腿
> 保持绿,publish-nightly 照常发出新种子,**无需手动传种子**。仅纯 download-bootstrap 的
> `vm-jit` / `stdlib-jit`(用旧 nightly 的旧 VM)仍会 bump 当次一次性红,下一 run 下到
> 新 nightly 自愈(它们不 feed publish-nightly,不阻塞)。下面描述的是这类**残留一次性红**。
>
> **`bench-regression` 不在此列**：它在 bump PR 上的红
> 根本不是「旧 nightly」问题，而是 A/B 的 base 侧结构上不可测（base stdlib 被 PR 的 z42.package 写成
> PR 格式、base VM 读不了）。检测到格式代差即**跳过 A/B 并打 warning**，不亮红。
> 原理见 book `dev/benchmarking.md`「跨格式代际的 PR 不做 A/B」。

CI 的 `xtask-bootstrap` composite **下载上一次 nightly**（`install-z42` → `.z42/`）来编译 + 运行 xtask（vm-jit 等 job）。所以 zbc/zpkg minor bump 后会短暂出现循环：

- 旧 nightly 的 z42vm 是旧 zbc reader → 跑不了用**新** z42c 编出的 `xtask.zpkg`（strict-pin 失败）；且 xtask 对着 `.z42/libs`（旧 nightly stdlib）编译，新 stdlib API 也可能缺。
- 于是 vm-jit **红**，直到存在兼容的新 nightly——而产出它的正是 `publish-nightly`。

**为什么不死锁（自愈设计）**：`publish-nightly` 的 `needs` **只含从当前源码构建的 job**（`build-and-test` 用 cargo + z42c 从源码 bootstrap xtask；`package-*` 用源码 `xtask build`），**绝不依赖 download-bootstrap 的 vm-jit / bench**。所以 bump commit 推上 main 后：源码 job 全绿 → publish-nightly 发布新 nightly → 下一次 run 的 vm-jit / bench 下到新 nightly → 自愈。bump 当次那一跑 vm-jit/bench 红是预期的、一次性的。

> **硬约束**：任何 feed `publish-nightly` 的 job 必须从**当前源码** bootstrap（不许走 download-nightly composite），否则 publish 路径变成依赖旧 nightly，死锁复活。
>
> 这正是 [bootstrap-seed.md](bootstrap-seed.md) "分阶段引入新语法 / 格式" 纪律要解决的问题：format bump 不要踩在会让旧 nightly 读不了当前源码的时机。

**手动发布 nightly（escape hatch）**：若自愈不及时（或要在不推 commit 的情况下刷新 nightly），手动触发 CI 的 `workflow_dispatch`，从当前 main 源码构建并发布 nightly：

```bash
gh workflow run CI --ref main          # 或 Actions 页面 "Run workflow"
```

`publish-nightly` 的 `if` 已放行 `workflow_dispatch`；vm-jit/bench 即使红也不挡发布。

---

## 编译器语义指纹（非格式失效次元）

> 触发条件：改了 **z42c 的 codegen / 优化 pass / typecheck / lowering 行为**，但 **zbc/zpkg
> 格式 Minor 没有 bump**（即同一份源码、同样的 wire 格式，编出的 `.zbc` 字节却会变）。

### 为什么需要它（与 zbc/zpkg 版本正交）

增量编译 cache 的失效判据（`.meta` / `package.meta`）只 pin `源内容 SHA-256 +
zbc/zpkg 格式 Minor`。这两者都**测不出"编译器语义变了但格式没变"**：多数 codegen / 优化
改动不动 wire 格式 → 格式 Minor 不 bump → `ProbeFiles` 命中旧 cache → **静默复用旧 `.zbc`
产物、不重编**，产物与当前编译器语义不一致。`CompilerFingerprint` 就是补的这个失效次元。

### bump 规则

| 情形 | 动作 |
|------|------|
| 改 codegen / 优化 pass / typecheck / lowering / **发出的诊断**，**且不 bump zbc/zpkg 格式** | 在 `CompilerFingerprint.Entries` **末尾追加一行本次变更的 slug** |
| bump 了 zbc/zpkg 格式 Minor | **不必**追加——格式 Minor 变化已让所有旧 `.meta` 失效（追加了也无害）|
| 只修 reader/writer 非格式 bug（不改 wire、不改编出的字节、不改诊断） | 不追加 |

> 🔴 指纹 =
> `CompilerFingerprint.Entries` 这张列表的**内容哈希**，没有手工计数器：
> 没有号可抢；两个 PR 各追加一行，合并只会**两行都留下**。
> 机制与各条目理由见
> [编译器语义指纹](../../internals/src/compiler/compiler-fingerprint.md)。

> ⚠️ **第 1 行明确含「发出的诊断」**。「发码没变」不是不必追加的理由 ——
> **诊断变了、字节没变**恰恰是最需要失效的一档：那类源文件哈希一字未变，
> 不失效就会命中旧条目、把新诊断整个吞掉。
>
> ⚠️ 第 3 行的主语是 **reader/writer**，不是「字节没变」—— 别把括号里的条件当成独立判据去
> 和第 1 行对撞。

**坐标**：`src/compiler/z42c.pipeline/src/CompilerFingerprint.z42` 的 `Entries`
（**只许在末尾追加**，不许改动/删除既有条目）。`CacheStore.Fingerprint()` 只是它的取值口。它进 `.meta` 的 `z42c-fp` 行与 `package.meta` 头；`Parse` / `LoadSrcList`
校验不符即令条目作废。**纯 z42c 内部格式，不涉 wire、不触发 zbc/zpkg 格式 bump、不需改 Rust 端。**

### CI 守门：输出变了就必须累加

「该不该 bump」由 CI 守门判断：bench-pr 工作流的 **Compiler fingerprint guard** 步骤用 base 编译器和
PR 编译器各编一遍**同一份 base stdlib 源码**，逐包比 zpkg 字节（同源同编译器 ⇒ 逐字节一致）。

| 输出字节 | 编译器身份（`Entries` / 格式 Minor） | 结果 |
|---------|-----------------------------------|------|
| 不变 | — | ✅ |
| 变了 | 至少一个变了 | ✅ |
| 变了 | 都没变 | ❌ 报出哪些包变了 → 去 `Entries` 末尾追加一行 slug |

> 🔴 **这道门对「诊断变了、发码不变」那一档是结构性地瞎的** —— 它比的是**产物字节**，而诊断
> 不进产物。那一档只能靠人按上表第 1 行记一条；门禁不会替你发现。「不变」一格并不等于纯重构：
> **只改诊断也落在这一格**。

本地复现：`xtask test compiler fingerprint --base <base 源码树根>`（base 树的 stdlib 须先由 base 编译器建好）。
覆盖面 = stdlib 实际走到的编译器路径；stdlib 没用到的 codegen 分支测不到（只会漏判，不会误判）——
这类改动仍按上表手动 bump。

