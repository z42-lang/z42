# Tasks: bench 门禁的 Rust 依赖缓存改在 main 上预热

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`ci`（缓存策略）→ 最小化模式。CI 简化第二批的「bench-pr 缓存 main 预热」。

## 问题

`bench-pr.yml` 的 `Swatinem/rust-cache`（`shared-key: bench-ab-v1`）只在 PR 上跑。GitHub 缓存的作用域：
PR 分支存的缓存只有该 PR 能读，main 上存的才对所有 PR 可见。实测 `gh cache list`：#975–#987 每个 PR 各存一份
**key 完全相同的 172 MB**（`v0-rust-bench-ab-v1-Linux-x64-6ff13d87-ff862451`）——每个新 PR 都冷编一轮，且这些副本
挤占仓库 10 GB 配额，把 main 上 CI 主缓存挤出去。

## 方案

- `bench-pr.yml` 增加 `push`（main；`src/runtime/**`、`.cargo/**`、本 workflow，排除 `*.md`）/ 每周 `schedule`
  （兜 rustc stable 升级——缓存 key 含 rustc 版本）/ `workflow_dispatch` 触发。
- 新 job `bench-cache-warm`（非 PR）：Rust 配置与 `bench-regression` 逐字相同，编门禁实际会编的三组产物
  （z42vm + z42-compression + `gc_cycle_bench` bench profile），由 rust-cache 在 main 上存。
- `bench-regression` 只在 PR 上跑，rust-cache `save-if: false`。
- concurrency 组：PR 每个一组；非 PR 共用 `warm` 一组。

## 进度概览

- [x] `.github/workflows/bench-pr.yml`
- [x] 文档：`benchmarking.md` §9
- [x] 本地 GREEN（基底 18a2ccee5，10m05s）
- [ ] PR CI（本 PR 改了 bench-pr.yml ⇒ bench-regression 会跑；确认 PR 侧不再回存）
- [ ] 合入后 main 上 bench-cache-warm 跑通并存出缓存；之后新 PR 的 rust-cache 命中
