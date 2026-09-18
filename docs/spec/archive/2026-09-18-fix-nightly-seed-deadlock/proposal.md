# Proposal: 破除 nightly 种子死锁

> 状态：🟡 进行中 | 创建：2026-09-18

## Why

**2026-09-17 全仓 CI 死锁，靠现有逃生口救不回来。**

事故链：

1. push `99710f20` 的 CI 跑到 `publish-nightly` 的 `gh release delete nightly` →
   `gh release create nightly` 中途，被下一个 push **取消**整个 workflow
2. 留下一个**残缺的 Draft nightly**（4 个资产，全部 SDK 包缺失）
3. 之后每轮 CI 的 bootstrap 都 `gh release download nightly -p z42-sdk-nightly-<rid>`
   拿不到东西 ⇒ `compile-toolchain` / `test-host` **全平台挂**
4. 挂了 ⇒ `package-*` / `publish-nightly` 全被 skip ⇒ **发不出新 nightly 自救**

`ci.yml:1416` 的 "NIGHTLY-SEED DEADLOCK POSTURE" 注释预见了这个形状，并指了逃生口
`gh workflow run CI --ref main`。**实测无效**（run 35287940676）：`publish-nightly` 的
`needs` 里是 `build-and-test` / `host-package` / `package-*` / `toolchain-bootstrap`，
而这些**全都要从旧 nightly 自举**。旧 nightly 坏了它们就全挂，publish-nightly 照样 skip。

⇒ **逃生口与它要逃的环在同一个环里。**

## 根因不止一个，要分开看

| # | 问题 | 性质 |
|---|---|---|
| A | **种子来源只有 `nightly` 一个，没有回退** | 单点故障。nightly 一坏，全仓失去自举能力 |
| B | **`publish-nightly` 会被整 workflow 取消打断** | `concurrency.cancel-in-progress: false` 只序列化该 job 的**并发**，挡不住新 push 取消**整个 run**。delete→create 之间被砍就留 stuck-draft |

A 是「坏了修不回来」，B 是「为什么会坏」。**本 change 修 A**（恢复能力），B 另记。

## What Changes

给 `ci-bootstrap` 的种子下载加**回退链**：

```
nightly（首选，10 次重试）
  ↓ 拿不到 / 包里没有 programs/z42c/
最近 5 次成功 CI 运行的 z42-host-package-* artifact（逐个试）
  ↓ 都过期 / 没有本 RID
报错退出（附人工恢复指引）
```

依据：**自举纪律保证「上一版 z42c 永远能编当前源码」**
（`bootstrap-seed.md` 的分阶段引入：support 先行、晚一个 nightly 再 use）。
成功 CI 运行的产物顶多落后一两个 commit，牢牢在这条单向递推的纪律内。

> 🔴 **第一版回落到「最近的正式 release」，实测救不回来**，已推翻。
> 正式 release 可能落后很多个 zpkg 格式 bump（v0.5.0 是 minor 43、当时源码 48，差 5 个），
> 会触发 [1.5] 两代自举；而两代自举全程用种子自带的旧 VM，它加载不了 gen1 产出的
> 新格式 stdlib —— run 35290607109 停在
> `z42.core.zpkg … zpkg minor 48 not supported (writer is at 0.43)`。
> CI artifact 格式天然一致，**根本不进两代路径**；顺带免掉 tar/zip 与 EXT 分支
> （artifact 里是已解包的 SDK 目录）。

这样即使 nightly 彻底损坏，CI 仍能自举 → `publish-nightly` 能跑 → **自动发出健康的
nightly，下一轮自愈**。死锁被破除。

## Scope

| 文件路径 | 变更类型 | 说明 |
|---|---|---|
| `.github/actions/ci-bootstrap/action.yml` | MODIFY | 种子下载加 CI-artifact 回退 |
| `.github/workflows/ci.yml` | MODIFY | 更新 "DEADLOCK POSTURE" 注释：逃生口为什么此前无效、现在靠什么破环 |
| `docs/internals/src/devinfra/ci.md` | MODIFY | 记录种子回退链与 stuck-draft 的处置 |
| `docs/spec/changes/fix-nightly-seed-deadlock/**` | NEW | 本 change |

## Out of Scope

- **B（publish-nightly 被取消打断）** —— 需要改 workflow 级 concurrency 或把 publish 拆成
  独立 workflow，与本 change 的「恢复能力」正交。已在 tasks 备注里登记，建议单开。
- 让 `publish-nightly` 不依赖 package 产物（那是另一套架构）。

## Open Questions

- [x] 回退目标选谁？→ **CI artifact，不是正式 release。**

      这一条我第一次答错了，原文是：「正式 release 更旧、跨度更大，但同样在纪律覆盖范围内；
      真正的风险是跨了 zbc/zpkg 格式 bump——那种情况下 nightly 也救不了，属另一类事故。」

      **前半句对，后半句错。** 格式 bump 不是「另一类事故」，它就是这条回退路径的**常态**：
      pre-1.0 阶段 zpkg minor 涨得比发版快，任意两个正式 release 之间隔好几个 bump 是常见情形。
      我把自己识别出的风险当成小概率外部条件排除掉了，实际它 100% 命中（实测 43 vs 48）。

      正确判据不是「在不在纪律覆盖范围内」，而是「**种子的 zpkg minor 是否等于当前源码**」——
      不等就进两代自举，而两代自举只扛得住一个 bump。CI artifact 满足这个判据，正式 release 不满足。
