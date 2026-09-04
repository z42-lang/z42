---
paths:
  - "src/compiler/z42c.ir/src/BinaryFormat/**"
  - "src/compiler/z42c.project/src/**"
  - "src/runtime/src/metadata/**"
  - "docs/design/runtime/zbc.md"
  - "docs/design/runtime/zpkg.md"
  - "src/tests/zbc-format/**"
  - "src/tests/zpkg-format/**"
---

# `.zbc` / `.zpkg` minor version bump checklist

> z42 pre-1.0 **strict-pin** 政策：Rust reader 精确匹配 writer 的 major + minor，无兼容回退。
> 兼容性原则（"不为旧版本提供兼容"）见 [philosophy.md](philosophy.md#不为旧版本提供兼容2026-04-26-强化)。
>
> 这份文件只回答一个问题：bump version 时**具体要同步改哪些文件**才能让 strict-pin 不变量 + golden 门通过。
> z42c（编译器）是 writer，z42vm（Rust）是 reader，两端版本常量必须同 commit 一起改。

---

## 版本常量坐标（唯一真相表）

> **路径注**：IR/zbc/zpkg 后端已由 `converge-z42c-ir-metadata-onto-stdlib` 从 `src/compiler/z42c.ir`
> / `z42c.project` 下沉到 stdlib 库 **`src/libraries/z42.ir`**（namespace 不变）。下表已用收敛后真实路径。

| 端 | 文件 | 常量 | 当前值 |
|----|------|------|--------|
| zbc writer（z42c） | `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` | 1 / 35 |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` | 1 / 35 |
| zpkg writer（z42c） | `src/libraries/z42.ir/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` | 0 / 40 |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` | 0 / 40 |

> reader 端（`zbc_reader.rs`）每个常量旁有逐行 minor changelog 注释（日期 / spec / 格式变化）——bump 时在那里追加一行。
> writer 端常量旁也有同样的单行 bump 注释，保持格式一致。

---

## Bumping `.zbc` minor version

修改 `.zbc` wire format（新 opcode / 新 section / 已定义 section 字段语义变化）时，**单次 commit 必须同步以下 5 处**，否则 Rust reader strict-pin 校验、`zbc_compat` 字节基线、或 z42c golden hex 单测任一会 fail：

1. **`ZbcFormat.z42`**（`src/libraries/z42.ir/src/BinaryFormat/`）— `ZbcVersion.Minor++`，常量旁注释本次 bump 内容（参考已有行格式）。若 bump 改了指令/section 布局，`ZbcInstr.z42`（编码）+ `ZbcReaderInstr.z42`（解码）或 `ZbcWriter.z42` 的对应 `Build*` / `_assemble` 逻辑同步。
2. **`zbc_reader.rs`**（`src/runtime/src/metadata/`）— `ZBC_VERSION_MINOR` 同步到新值；并在常量上方 changelog 注释块追加一行（日期 / spec / 字段变化）；reader 解码逻辑（`read_*_section`）同步新格式。
3. **`docs/design/runtime/zbc.md`** — "Minor changelog" 表加一行（minor / 日期 / 触发 spec / 引入内容）。
4. **regen zbc-format fixture** — 跑 `xtask build test`（前置 `build compiler`+`build stdlib` 已用新格式重建），原地覆写 `src/tests/zbc-format/*/source.zbc`（6 个 committed 字节基线：`empty` / `strp-func-minimal` / `multi-method` / `with-tidx` / `cross-import-token` / `with-frcs`）；`git diff` 应显示格式 delta，**必须连同 bump 一起提交**。

   > 🔒 **CI 有门（`refresh-format-fixtures`，2026-09-04 起）**：`compile-test-assets` job 在 `build test`
   > 之后跑 `git diff --quiet -- src/tests/zbc-format`，**有差异即红**。
   >
   > 为什么需要这道门：此前这些基线唯一的把关方式是「人工注意到 `git diff`」。而 regen 在所有消费者
   > **之前**就地覆写，于是 `zbc_compat` 校验的永远是刚重生的字节、**从不是 committed 的那份** ——
   > 陈旧基线因此可以一直绿着，同时把每个人的工作树弄脏。实际后果：这 6 个 fixture 停在 zbc **1.37**
   > 一路熬过了 1.38 的 bump（#414 漏了本步），2026-09-04 才被发现。形态同 `cargo fmt --check`。
5. **z42c golden hex 单测** — `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` 的 `test_zbc_empty_byte_identical` 内嵌 `empty/source.zbc` 的 hex 串（zbc 1.21 起 226B；header 的 `minor` 字段 + STRS 段体会随 bump 变化）。从 regen 后的 fixture 重截：
   ```bash
   xxd -p src/tests/zbc-format/empty/source.zbc | tr -d '\n'
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

**zbc minor bump 必须同步 bump zpkg minor**（zpkg 内嵌 zbc，见 `docs/design/runtime/zpkg.md`）。在上述 5 步外加：

6. **`ZpkgWriter.z42`**（`src/libraries/z42.ir/src/`）— `ZpkgWriterZ.Minor++`，注释更新内嵌 zbc 版本。
7. **`zbc_reader.rs`** — `ZPKG_VERSION_MINOR` 同步；上方 zpkg changelog 注释块追加一行（指明耦合的 inner zbc minor）。
8. **`docs/design/runtime/zpkg.md`** — Minor changelog 加一行（触发 spec = 同次 zbc bump 的 spec）。
9. **regen zpkg-format fixture** — 覆写 `src/tests/zpkg-format/*/source.zpkg`（4 个 committed 基线：`packed-minimal` / `packed-multi-module` / `indexed-minimal` / `sym-only-sidecar`）。

   > ⚠️ **zpkg-format 暂无一键 regen**：`xtask build test` 目前只覆盖 zbc-format，zpkg fixture 需手工用 `z42c build` 逐个重生覆写（见 `src/tests/zpkg-format/README.md` TODO）。

提交前自检扩展：

```bash
cargo test lazy_loader          # Rust reader 读 committed zpkg 字节基线
```

---

## Bumping `.zpkg` minor version（independent）

仅改 zpkg outer（不动 zbc）时（如新增 zpkg-only section / 已定义 section 字段语义）：只触步骤 6–9（zpkg writer / Rust 常量 / zpkg.md changelog / zpkg fixture regen），跳过 zbc 步骤 1–5。

注意：实际工作中 zpkg-only 改动非常罕见（历史上所有 minor bump 都耦合 zbc），但若发生，本节给出独立路径。

---

## 本地全量验证 / fixture 重生的配方（格式 bump 专用，2026-09-02 验证）

> **这条解决一个长期的假前提**：过去认为「引入新格式后本地无法全量验证、fixture 无法本地重生 →
> 只能靠 CI」（因为本地两代自举在 macOS 撞环境墙）。**其实有干净解法**——让 CI 先把新格式工具链建出来、
> 下载回本地当种子。`fix-generic-array-value-zero-init`（zbc 1.37/zpkg 0.42）用它在 macOS 本地跑通了
> 完整 GREEN + fixture 重生，无需两代自举。

### 为什么本地直接建不动

minor bump 后，本地 `cargo build` 出的 z42vm 是**新格式**（reader 钉新 minor），但本地唯一的 z42c/stdlib
种子（`.z42/` 下载的 nightly、或 warm `artifacts/`）还是**旧格式**。warm 建 `xtask build compiler/stdlib`
→ 新 VM 读旧种子 zpkg → `zpkg minor <旧> not supported (writer is at <新>)` 直接墙掉。要把种子推进到新格式
本需**两代自举**（旧 VM 跑 gen1/gen2），而本地两代自举在 macOS 有独立的环境墙（见
`bootstrap-seed.md`）。→ 死结。

### 解法：下载 CI 建好的新格式工具链当本地种子

CI 的 `compile-toolchain` job（两代自举已根治）从**当前 PR 源码**建出新格式的 z42c + 全 stdlib，并
`upload-artifact` 为 `toolchain-<os>`（`toolchain-macos-15` / `toolchain-ubuntu-latest`）。把它下回本地
overlay 成种子，**种子与 cargo VM 就同为新格式** → warm 建/测/regen 全通，两代自举彻底不需要。

> zpkg 是可移植字节码——linux 建的 z42c.driver.zpkg 也能在 macOS cargo VM 上跑；有同-OS artifact 优先用。

**步骤**（承接上面「Bumping」各步已改完源码 + 版本常量）：

1. **先推一个 PR**。首轮 CI：`compile-toolchain` 应绿（新格式工具链建成 + 上传）；`test-host` 预期**红在
   committed fixture**（旧格式，还没重生）——正常，用这轮只为拿工具链 artifact。
2. **下载 + overlay**（保留你自己的 `runtime/z42vm`）：
   ```bash
   RUN=<compile-toolchain 所在 run-id>          # gh run list --branch <your-branch>
   gh run download $RUN -n toolchain-macos-15 -D /tmp/tc
   rm -rf artifacts/build/compiler artifacts/build/libraries
   cp -R /tmp/tc/artifacts/build/compiler   artifacts/build/
   cp -R /tmp/tc/artifacts/build/libraries  artifacts/build/
   cp    /tmp/tc/artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zpkg
   ```
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
   LIBS=$PWD/artifacts/build/libraries/dist/release
   # 临时工程：name 匹配 expected.json（demo.minimal/demo.multi/demo.indexed），kind=lib
   #   packed → --release；indexed → 无 --release（另产散装 source.zbc）
   Z42_LIBS="$LIBS" "$VM" "$Z42C" -- build <temp>/demo.minimal.z42.toml --release
   cp <temp>/dist/demo.minimal.zpkg src/tests/zpkg-format/packed-minimal/source.zpkg
   # indexed 另拷 dist/source.zbc → indexed-minimal/source.zbc（散装 + FILE 段 hash 自动同步）
   ```
   `sym-only-sidecar` 无 Rust 字节读测试 → 保持旧格式不动（沿 f9928607/58d04cb7 处置）。
5. **重生的 fixture + 收尾 commit** 一起 push；PR 转全绿后合并。

### 与旧记录的关系

- 取代 `escape-stack-format-bump-ci-learnings` §3 的「加临时 CI 步骤重生 fixture」绕法——直接下载
  `compile-toolchain` 已上传的工具链更省事，无需改 workflow。
- `bootstrap-seed.md` 的「macOS 本地两代自举环境墙」依然存在，但**本配方绕开了它**（不再需要本地两代
  自举，改用 CI 建好的种子）。

---

## bump 与 xtask↔nightly bootstrap 循环

> **✅ 格式-bump 死结已根治（2026-07-09，fix-bootstrap-format-bump-deadlock）**：ci-bootstrap
> 加了**版本差 gate + 两代自举**——种子 minor ≠ 当前 writer minor 时,用 nightly SDK 自带的
> **旧 VM**(bin/z42vm)跑 gen1/gen2 把种子推进到当前格式,再交 cargo 新 VM。所以 **zpkg/zbc
> minor bump 后 build-and-test / host-package / verify-selfhost 等**从当前源码 bootstrap 的腿
> **不再全红**,publish-nightly 照常发出新种子,**无需手动传种子**。仅纯 download-bootstrap 的
> `vm-jit` / `stdlib-jit` / `bench`(用旧 nightly 的旧 VM)仍会 bump 当次一次性红,下一 run 下到
> 新 nightly 自愈(它们不 feed publish-nightly,不阻塞)。下面描述的是这类**残留一次性红**。

CI 的 `xtask-bootstrap` composite **下载上一次 nightly**（`install-z42` → `.z42/`）来编译 + 运行 xtask（vm-jit / bench 等 job）。所以 zbc/zpkg minor bump 后会短暂出现循环：

- 旧 nightly 的 z42vm 是旧 zbc reader → 跑不了用**新** z42c 编出的 `xtask.zpkg`（strict-pin 失败）；且 xtask 对着 `.z42/libs`（旧 nightly stdlib）编译，新 stdlib API 也可能缺。
- 于是 vm-jit / bench **红**，直到存在兼容的新 nightly——而产出它的正是 `publish-nightly`。

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

## 编译器语义指纹（非格式失效次元，2026-08-11 add-compiler-fingerprint-cache）

> 触发条件：改了 **z42c 的 codegen / 优化 pass / typecheck / lowering 行为**，但 **zbc/zpkg
> 格式 Minor 没有 bump**（即同一份源码、同样的 wire 格式，编出的 `.zbc` 字节却会变）。

### 为什么需要它（与 zbc/zpkg 版本正交）

增量编译 cache 的失效判据（`.meta` / `package.meta`）此前只 pin `源内容 SHA-256 +
zbc/zpkg 格式 Minor`。这两者都**测不出"编译器语义变了但格式没变"**：多数 codegen / 优化
改动不动 wire 格式 → 格式 Minor 不 bump → `ProbeFiles` 命中旧 cache → **静默复用旧 `.zbc`
产物、不重编**，产物与当前编译器语义不一致。`CompilerFingerprint` 就是补的这个失效次元。

### bump 规则

| 情形 | 动作 |
|------|------|
| 改 codegen / 优化 pass / typecheck / lowering，**且不 bump zbc/zpkg 格式** | `CacheStore.CompilerFingerprint++`（**唯一**要动的地方）|
| bump 了 zbc/zpkg 格式 Minor | **不必**动指纹——格式 Minor 变化已让所有旧 `.meta` 失效（动了也无害）|
| 只修 reader/writer 非格式 bug（不改 wire、不改编出的字节） | 不动指纹 |

**坐标**：`src/compiler/z42c.pipeline/src/CacheStore.z42` 的 `CacheStore.CompilerFingerprint`
（常量旁有注释）。它进 `.meta` 的 `z42c-fp` 行与 `package.meta` 头；`Parse` / `LoadSrcList`
校验不符即令条目作废。**纯 z42c 内部格式，不涉 wire、不触发 zbc/zpkg 格式 bump、不需改 Rust 端。**

> follow-up（roadmap Deferred）：自动聚合 z42c zpkg `build_id` 作指纹，令编译器一变即自动失效、
> 免人肉 bump（B 方案，见 `add-compiler-fingerprint-cache` 归档备注）。
