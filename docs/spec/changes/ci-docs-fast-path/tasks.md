# Tasks: CI 纯文档快速通道 + 单一结论 job `ci-ok`

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`ci`（门控与编排）→ 最小化模式。CI 简化第二批的「docs 快速通道」「ci-ok 聚合 job」两项。

## 问题

1. **纯文档 PR 跑全套**：`docs/**` / `*.md` 早已不在 paths-ignore（死链门必须跑到），但代价是 test-host ×4
   （Windows 腿最慢）+ compile-toolchain → compile-test-assets → test-consume 全跑一遍——而读文档的门禁只有
   `test docs`（相对链接）与 `test diagcodes`（诊断码文档），外加 `xtask test` 开头那道 gate stage ↔ test-gate.md 对账。
2. **分支保护逐个列 job**：test-host ×4 + verify-features + compiler-checks。新增 / 改名 job 时容易漏；`verify-selfhost`
   删除后保护里还挂着它，PR 一直等不到这个 check（本轮早些时候实际发生）。

## 方案

- `detect-changes` 加 `Docs-only PR?`：取 PR 文件列表逐个判（全称判断，paths-filter 的默认语义是「任一命中」）；
  `docs/learn/**`、`examples/**` 不算文档（示例会被重放）。只在 PR 上判。
- 新 job `docs-check(linux-x64)`（仅 docs_only）：ci-bootstrap 新增 `xtask-only` 输入——种子 z42c 编出 xtask 即停，
  xtask 跑在种子 stdlib 上（`Z42_LIBS` 经 `GITHUB_ENV` 交给垫片）——然后 `xtask test docs` + `xtask test diagcodes`。
- `test-host` / `compile-toolchain` 在 docs_only 时 skip（下游 assemble / consume 随之 skip）。被 `if:` skip 的
  required check 视同通过。
- `xtask test docs` 并入 ② gate stage 清单 ↔ test-gate.md 对账（此前只在 `xtask test` 开头查，快速通道会漏）。
- 新 job `ci-ok`：`needs` 全部 job、`if: always()`，任一 failure / cancelled 即红。分支保护改为只要求它
  ——**需要仓库管理员操作**（本 PR 不改保护）。

## 进度概览

- [x] `.github/workflows/ci.yml`：Docs-only 判定 / `docs-check` / 门控 / `ci-ok`
- [x] `.github/actions/ci-bootstrap/action.yml`：`xtask-only`
- [x] `scripts/test/xtask_test_docs.z42`：② gate stage 对账
- [x] 文档：`ci.md`（触发与门控、job 表、ci-ok）
- [x] 本地 GREEN（基底 18a2ccee5，8m21s；首轮被 ci-shell 门拦下——它不认 step env: 与 `while read`，改为 `${X:-d}` / `${X:?}` 与 grep 集合判断）
- [ ] PR CI（改了 ci.yml ⇒ 全跑；docs_only=false 路径）
- [ ] docs_only=true 路径实测：开一个纯文档 PR（如本系列的归档 PR）确认只跑 docs-check、ci-ok 绿
- [ ] 分支保护改为只要求 `ci-ok`（管理员）
