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

| 端 | 文件 | 常量 | 当前值 |
|----|------|------|--------|
| zbc writer（z42c） | `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | `ZbcVersion.Major` / `.Minor` | 1 / 20 |
| zbc reader（Rust） | `src/runtime/src/metadata/zbc_reader.rs` | `ZBC_VERSION_MAJOR` / `_MINOR` | 1 / 20 |
| zpkg writer（z42c） | `src/compiler/z42c.project/src/ZpkgWriter.z42` | `ZpkgWriterZ.Major` / `.Minor` | 0 / 23 |
| zpkg reader（Rust） | `src/runtime/src/metadata/zbc_reader.rs` | `ZPKG_VERSION_MAJOR` / `_MINOR` | 0 / 23 |

> reader 端（`zbc_reader.rs`）每个常量旁有逐行 minor changelog 注释（日期 / spec / 格式变化）——bump 时在那里追加一行。
> writer 端常量旁也有同样的单行 bump 注释，保持格式一致。

---

## Bumping `.zbc` minor version

修改 `.zbc` wire format（新 opcode / 新 section / 已定义 section 字段语义变化）时，**单次 commit 必须同步以下 5 处**，否则 Rust reader strict-pin 校验、`zbc_compat` 字节基线、或 z42c golden hex 单测任一会 fail：

1. **`ZbcFormat.z42`**（`src/compiler/z42c.ir/src/BinaryFormat/`）— `ZbcVersion.Minor++`，常量旁注释本次 bump 内容（参考已有行格式）。若 bump 改了 section 布局，`ZbcWriter.z42` 的对应 `Build*` / `_assemble` 逻辑同步。
2. **`zbc_reader.rs`**（`src/runtime/src/metadata/`）— `ZBC_VERSION_MINOR` 同步到新值；并在常量上方 changelog 注释块追加一行（日期 / spec / 字段变化）；reader 解码逻辑（`read_*_section`）同步新格式。
3. **`docs/design/runtime/zbc.md`** — "Minor changelog" 表加一行（minor / 日期 / 触发 spec / 引入内容）。
4. **regen zbc-format fixture** — 跑 `xtask build test`（前置 `build compiler`+`build stdlib` 已用新格式重建），原地覆写 `src/tests/zbc-format/*/source.zbc`（6 个 committed 字节基线：`empty` / `strp-func-minimal` / `multi-method` / `with-tidx` / `cross-import-token` / `with-frcs`）；`git diff` 应显示格式 delta。
5. **z42c golden hex 单测** — `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` 的 `test_zbc_empty_byte_identical` 内嵌 `empty/source.zbc` 的 247B hex 串（header 的 `minor` 字段会随 bump 变化）。从 regen 后的 fixture 重截：
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

6. **`ZpkgWriter.z42`**（`src/compiler/z42c.project/src/`）— `ZpkgWriterZ.Minor++`，注释更新内嵌 zbc 版本。
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
