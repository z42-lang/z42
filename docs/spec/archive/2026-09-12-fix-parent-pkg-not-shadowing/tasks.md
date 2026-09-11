# Tasks: fix-parent-pkg-not-shadowing

> 状态：🟢 已完成 | 创建：2026-09-12 | 类型：fix（E0606 回归，main 当前判红）

**变更说明：** #577 的 E0606（本包遮蔽导入包同 FQN 类型）在 **dev-target 模式**下误报，
导致 `xtask test targets` 的 `dev-target-internal` 判红——**main 当前 CI 三平台 `test-host` 全挂**。

## 误报的形状

dev-target 模式（z42b-owns-test-targets）为了让测试目标看见父包的 `internal`，把**父包的源码**
编进测试单元；而父包自己的 zpkg **同时**在依赖里。于是父包的每一个类型都长成
「本地声明 + 导入同 FQN」⇒ E0606 对每一个都报：

```
tests/dirunit/source.z42(20,5): E0606: `Dt.Internal.Hidden` is declared in this package
  and also in package `dt.internal` — ...
```

**但那根本不是遮蔽，那就是同一个包。** `ImportedSymbolLoader` 里早就有这个概念——
`nct.IsImported = !(parentPkg != "" && pkgNames[i] == parentPkg)`：父包按**同包**待遇。
E0606 的来源登记漏了跟上这条。

## 修复

来源登记跳过父包：**你不可能遮蔽你自己**。常规编译 `parentPkg == ""` → 不跳（且那时 DepScan
已按 `inp.Name` 排除 self-zpkg，本就不自撞），故对既有诊断行为零影响。

## 🔴 它是怎么漏过 #577 的 GREEN 的（这条比修复本身重要）

| 时间 | 事件 |
|---|---|
| 09-12 01:51 | 我把 main（`63ebfe59`）并进 #577 分支并跑全量 GREEN → 全绿 |
| 09-12 04:49 | **#580 落地 main**，新增 `tests/dirunit/` 这个 fixture（正是照出误报的那个） |
| 09-12 06:36 | #577 被合并 —— 中间**没有任何东西重新验证过** |

§3 要求「合并前并入 main 最新改动 + 重跑 GREEN」。我做了，但做在**合并前几小时**，
而 PR 不是我按的合并键。⇒ **「验证完」与「合并」之间的窗口本身就是风险**，
窗口里落地的 PR 会让先前的 GREEN 结论过期。这与记忆里 `check-inflight-prs-before-starting`
记的是同一个坑（那次侥幸无事，这次真咬出了回归）。

**可操作的收口**：PR 不能立即合并时，① 在 PR body 里写明「本轮 GREEN 基于 main 的哪个 sha」；
② 合并前若 main 已前进，必须重跑再合。

## 验证

- [x] `xtask test targets` 恢复全绿（含 `dev-target internal`）
- [x] E0606 / E0601 的既有门仍全绿（单测 + 两个 cross-zpkg 负例 fixture）——修复没把门修哑
- [x] `xtask test` 全绿（13 stage）+ 自举不动点 3/3
- [x] 零格式 bump
- **回归门已存在**：`dev-target-internal` fixture 本身就是这道门（它抓到了这次误报），无需新建
