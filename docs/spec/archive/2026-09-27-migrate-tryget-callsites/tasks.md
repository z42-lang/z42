# Tasks: 取即检查的阶段 2 —— 迁 `ManifestLoader` 与 `xtask_bench` 的 91 处调用点

> 状态：🟢 已完成 | 创建：2026-09-27 | 完成：2026-09-27
> 分支/worktree：`migrate-tryget-callsites` @ `wt-tryget2` | 基于：origin/main `1730958e0` (#894)
> 类型：`refactor`（纯调用点重写，无 API 变更、无编译器改动、无格式 bump）
> 授权：#884 挂下的阶段-2 欠账（`scripts/test/stage2-debt.txt`），User「合并了，请你继续」

## 这是 #884 的阶段 2

#884 加了 `TryGet` / `TryGetValue<T>`（三个文档值类），但**两处调用点被自举纪律挡下**：

| 文件 | 处数 | 为什么阶段 1 做不了 |
|---|---|---|
| `z42.project/ManifestLoader.z42` | 76 | `ci-bootstrap` **[3/5]** 预建 `z42.project` 时链的是 **flat 已有的 `z42.toml`**，冷启动下 = **种子的** |
| `scripts/xtask_bench.z42` | 15 | **[2/5]** 种子 z42c 编当前 `xtask.zpkg` —— `scripts/` 全仓受自举约束最紧 |

阶段 1 的处置是回退 + 挂账。本刀 = 阶段 2：**API 已随 nightly 进种子** ⇒ 把迁移贴回去 + 清账。

## ✅ 前置条件（已核对）

**`TryGet` / `TryGetValue<T>` 必须已在 CI 拉的那个 nightly 种子里。** 判据两条，都过了：

- `gh release view nightly -q .publishedAt` = **2026-09-27T02:01:27Z**，
  晚于 #884 的 mergedAt（`2026-09-27T01:03:37Z`）✅
- 光看时间戳不够——还核了**血缘**：`git merge-base --is-ancestor 62c7b8986 4ce0b4fff` 成立，
  即发 nightly 的那棵树确实含 #884 ✅（时间戳只是代理指标，两个 run 并发时能骗人）

⚠️ **本机 GREEN 测不到这条**（#884 实录）：本地 flat 里躺的早就是当前源 stdlib，
预建一跑就对；只有 CI 冷启动才拿种子。⇒ 本机绿**不是**放行条件，nightly 时间戳才是。

## 迁移代码不是重写的

取自 #884 的第一个 commit `2920c2114`（squash 合并 ⇒ 不在 main 上）：

```
git fetch origin refs/pull/884/head:refs/remotes/origin/pr-884
git checkout origin/pr-884~1 -- src/libraries/z42.project/src/ManifestLoader.z42 scripts/xtask_bench.z42
```

⭐ **贴回去之前核过「期间没人动过这两个文件」**：
`git log origin/pr-884~2..origin/main -- <那两个文件>` 为空
（[[z42-stale-worktree-silent-revert]]：拿旧树内容覆盖会静默抹掉别人的 PR）。

## 进度概览

- [x] 1 贴回迁移（两个文件，`ContainsKey` 各归零）
- [x] 2 删掉两处 `STAGE2-DEBT(tryget-callsites)` 标记
- [x] 3 `xtask test stage2 --update` 清账 + 全量 GREEN
- [x] 4 **等 nightly 带上 API** → 开 PR

⚠️ 删标记时第一版**把 `///` 文档注释一起吃掉了**（判据写成「连续的 `//` 行」，而 `///` 也以 `//` 开头）。
正确判据：遇到 `///` 或非注释行就停。

## ⭐ 第一轮全量的唯一一条红，红得正确

15 个 stage 里只有 `stage2` 红：

```
✗ 阶段-2 欠账已清却还挂着: libraries/z42.toml/src/TomlValue.z42#tryget-callsites
    源里已无该标记 —— 清单留着就会腐坏成假的
```

我删了源里的标记却没跑 `--update` 清台账 —— **反向棘轮**抓到的正是这个。
这道门（#862 立的）第一次在真实场景里两个方向都验过了：#884 验了「新债不挂账 → 红」，
本刀验了「债清了不清账 → 红」。

## 收尾
- [x] 归档（阶段 9，在本 PR 内）→ `archive/2026-09-27-migrate-tryget-callsites/`
