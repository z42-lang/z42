# Tasks: 改 z42b 的 PR 也要跑 package-*

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`ci`（门控补洞）→ 最小化模式。

## 问题

z42b（`src/toolchain/builder/**`）只在 `stdlib` filter 里（它是 [Test] 执行器），不在 `platform` 里。而 z42b 的
`publish`（desktop apphost / iOS / Android 导出）**只在 `package-*` 里被执行** —— 在 PR 上它们只看 `platform`。

实例：#978 改了 z42b 的产物布局解析，Windows 上 apphost 的嵌入路径变成 `../../C:/...`；PR 上 package-host 被 skip，
合入 main 后 `package-host(windows-x64)` 的 desktop-publish smoke 才红（#981 修）。

## 方案

`platform` filter 加 `src/toolchain/builder/**`（与已在其中的 launcher / devtools / interactive 同形）。
顺带修正 ci.md 的 flag 表：`platform` 行还列着 `examples/**`、`docs/learn/**`，二者在 #957 已拆到独立的 `examples` flag。

## 进度概览

- [x] `.github/workflows/ci.yml`：`platform` 加 builder
- [x] `docs/internals/src/devinfra/ci.md`：flag 表同步 + 说明为什么 builder 两处都在
- [x] 本地 GREEN（基底 1fb724594，9m35s）
- [ ] PR CI（本 PR 改了 ci.yml ⇒ 全部 filter 命中、全跑）
