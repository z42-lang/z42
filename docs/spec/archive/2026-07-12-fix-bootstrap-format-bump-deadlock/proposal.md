# Proposal: 两代自举根治格式-bump 的 CI 引导死结

> 🔴 DRAFT，待 User 阶段 6.5 确认。这是 Deferred 条目
> `self-hosting-future-single-vm-bootstrap-gap` 的落地实现。
> 触发背景:add-indexed-zpkg-min-patch(zpkg 0.23→0.24)推 main 后 CI 15 腿全红,
> 靠手动发 0.24 nightly 种子(逐 SDK zpkg swap + `gh release upload --clobber`)才恢复。

## Why

1. **每次 zpkg/zbc minor bump 必然让 CI 全红,且不会自愈,需手动传种子。**
   `.github/actions/ci-bootstrap`(所有 build/test/package 腿都经它)只有**一个** VM——
   step [0/5] `cargo build z42vm`(新源码,新 minor reader)。step [2/5] 用这个新 VM 去跑
   下载的**上一版 nightly 种子**(旧 minor 的 z42c.zpkg + stdlib),strict-pin 精确匹配、
   无兼容回退 → 新 VM 读不了旧种子 → `zpkg minor N not supported (writer is at N+1)` → 全红。
2. **不会自愈**:CI 无 nightly cron(只 push 触发);ci-bootstrap 无版本容错;
   `publish-nightly` 的 needs(build-and-test/host-package/package-*/toolchain-bootstrap)
   全经这个红掉的 bootstrap → 发不出携带新格式的 nightly → 下次 push 种子仍是旧的 → 死循环。
   ci.yml publish-nightly 注释亲述该后果:"publish can't republish a fixed nightly → manual recovery"。
3. **strict-pin 不能动**:它是仓库刻意选的(逼 regen、杜绝兼容代码、抓 writer/reader 漂移),
   philosophy.md 明确"不为旧版本提供兼容"。所以正解不是让 reader 兼容读旧版,而是
   **用旧 VM 把种子推进到新格式**——保持 strict-pin 原封不动。

## What Changes(核心:两代自举,旧 VM 在 SDK 里现成)

nightly SDK 每个 RID 都自带一个 `bin/z42vm`(上一版 VM,旧 minor reader),ci-bootstrap 当前
**没用它**。改为:

1. **检测版本差**:比较下载种子的 zpkg minor 与当前源码 writer minor。**相等**(日常 run)→
   走现有快路径(单 VM,零额外成本)。**不等**(格式 bump)→ 走下面的两代自举。
2. **两代自举**(仅 bump 时):
   - **Gen1**:用 SDK 自带的**旧 VM**(旧 minor)跑**旧种子 z42c** → 编当前 z42c 源 + 当前
     stdlib 源。产物是【旧格式外壳,但字节码逻辑=当前源=会写新 minor】——旧 VM 读得了自己
     产的旧格式外壳 ✓。
   - **Gen2**:旧 VM 跑 **Gen1 z42c**(旧格式,能被旧 VM 读)→ Gen1 逻辑写新 minor → 再编当前
     z42c + stdlib → 产物是【真正的新格式】z42c + stdlib。
   - **切换**:cargo 建新 VM(新 minor)→ 读 Gen2 的新格式产物,一致 → 正常 build/test/package。
3. **为什么必须两代**:Gen1 还是旧格式外壳(新 VM 读不了),Gen2 才是新格式外壳(新 VM 能读)。
   等价 GCC/rustc 的 stage0→stage1→stage2。

## Scope(允许改动的文件)

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `.github/actions/ci-bootstrap/action.yml` | MODIFY | 主改:版本差检测 + 两代自举分支(用 SDK 的 bin/z42vm 跑 Gen1/Gen2) |
| `scripts/build/xtask_bootstrap_check.z42`（如复用其 gen 逻辑）| MODIFY | 若把两代逻辑抽成 xtask 子命令供 action 调用 |
| `scripts/build/`（可能新增 `xtask bootstrap-twogen` 命令）| NEW? | 把"旧 VM + gen1/gen2"编排放进 xtask(比纯 bash 更可测/可维护;实施期定) |
| `.claude/rules/version-bumping.md` | MODIFY | "bump 与 xtask↔nightly bootstrap 循环"段:死结已由两代自举根治,删手动恢复告警 |
| `.claude/rules/bootstrap-seed.md` | MODIFY | single-vm-gap 已解:补两代自举机制说明 |
| `docs/design/compiler/self-hosting.md` | MODIFY | 关闭 Deferred `self-hosting-future-single-vm-bootstrap-gap` |
| `docs/roadmap.md` | MODIFY | Deferred 索引行更新 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | toolchain 锁登记/释放 |

**只读引用**:`.github/workflows/ci.yml`(publish-nightly needs / job 拓扑);
`.github/actions/ci-bootstrap/action.yml` 现有 5 步;nightly SDK 结构(bin/z42vm + programs/z42c + libs)。

## Out of Scope

- 语法/API 维度的分阶段引入(support 先行、晚一 nightly 再 use)——那是 bootstrap-seed.md
  已有的、正交的纪律;两代自举**依赖**它成立(旧 z42c 必须能编当前源),但不改它。
- strict-pin 本身(不动)。
- 移动端/wasm runtime 包(不经 z42c 种子)。

## Open Questions

- [ ] 两代编排放 bash(action.yml 内)还是抽 `xtask bootstrap-twogen`?后者可维护/可本地干跑
  (mock 种子),但要 xtask 已可运行(先有种子)——鸡蛋问题,可能仍需一小段 bash 引导。design 定。
- [ ] Gen1/Gen2 都要重建 stdlib 吗?Gen1 stdlib(旧格式)供 Gen1 z42c 自身依赖解析;Gen2 stdlib
  (新格式)供新 VM。需确认两代各自的 Z42_LIBS 指向。design 细化。
- [ ] 只 gate 在 minor 差,还是 major 差也覆盖?(major bump 更罕见,可能需人工。)design 定。

## 诚实的代价(必须在 design 展开)

1. **只能在 CI 上验证**:这条路走 download-nightly,本地无法完整跑(可用 mock 旧种子部分模拟,
   但真实性有限)。迭代慢、试错烧 CI 周期——这是它一直被 Defer 的主因。
2. **bump run 多两遍全量编译**(Gen1/Gen2 各编 z42c + stdlib,约几分钟)。非 bump run 零成本
   (版本差检测走快路径)。
3. **ci-bootstrap 复杂度上升**:两 VM 交错 + 分代,得仔细写、注释清楚。
