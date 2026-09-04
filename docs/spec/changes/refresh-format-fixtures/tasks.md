# Tasks: 刷新 zbc golden 字节基线 + 加防腐门（refresh-format-fixtures）

> 状态：🟢 PR-A 实施+验证完成 | 创建：2026-09-04 | 类型：fix + chore（不改产品行为 → 最小化模式）
>
> **变更说明**：把停在 zbc 1.37 的 6 个 committed 字节基线刷到当前 writer（1.38），并加一道 CI 门
> 让「基线陈旧」从静默变成红。
> **原因**：见下「根因」。**文档影响**：`.claude/rules/version-bumping.md` 第 4 步补记 CI 门与本次事故。

## 症状

`src/tests/zbc-format/*/source.zbc` 六个 committed 基线停在 **zbc 1.37**，而 writer 自 #414 起是 **1.38**。
后果：**任何一次构建都会就地重生它们、弄脏工作树**（`xtask build test` 是 regen 入口）。
在 PR #420 期间两次把它们误扫进无关提交，才引起注意。

## 根因（两层）

1. **checklist 第 4 步被跳过**：#414 做了格式 bump 的 1/2/3/5/6/7/8 步，漏了第 4 步（regen zbc fixture）。
   它 patch 了 `src/tests/zpkg-format/` 里被测试读的那两个（否则测试会红），zbc 这六个没人读、于是漏掉。
2. **这些基线从来没有被真正校验过**（真正的根因）：regen 在所有消费者**之前**就地覆写，于是
   `cargo test --test zbc_compat` 校验的永远是刚重生的字节、**从不是 committed 的那份**。
   唯一的把关方式是「人工注意到 `git diff`」——而基线长期陈旧恰恰把人训练成无视这个 diff，
   机制价值归零。**实证**：`zbc_tests.z42` 内嵌的 golden hex 已是 1.38、fixture 却是 1.37，测试照样绿。

## 实施

- [x] 1.1 用**当前源**重建工具链后 regen（`xtask build compiler` → `build stdlib` → `build test`），
      而非用 nightly z42c —— 基线必须由当前 writer emit
- [x] 1.2 6 个 fixture 全部 37→38，**每个恰好差 1 字节**（header 的 minor 字段）
      → 说明 1.37→1.38 之间这些 fixture 的 wire layout 未变，是纯版本号 delta
- [x] 1.3 **CI 防腐门**（`.github/workflows/ci.yml`，`compile-test-assets` job）：`build test` 之后
      `git diff --quiet -- src/tests/zbc-format`，有差异即红并打印修复指引。形态同 `cargo fmt --check`
- [x] 1.4 `version-bumping.md` 第 4 步补记该门 + 为什么需要它 + 本次事故

## 验证

- [x] 2.1 `cargo test --release --test zbc_compat` 3 passed（读 committed 基线）
- [x] 2.2 `xtask test compiler` 绿：内嵌 golden hex 单测通过（**hex 与 regen 后 fixture 逐字节一致**，
      印证 #414 只同步了 hex 没同步 fixture）+ 自举不动点 3/3 gen1==gen2
- [x] 2.3 `xtask test` 完整 GREEN（all stages passed；行数门 0 new/grown）
- [x] 2.4 提交后工作树对 `src/tests/zbc-format` 应为零 diff → 新门在后续 PR 上会保持绿

## 后续（PR-B，另开）

审计发现 zbc 之外还有更陈旧的：

| fixture | 版本 | 状况 |
|---|---|---|
| `zpkg-format/packed-multi-module/source.zpkg` | ZPK **42**（当前 43）| 无任何测试读 |
| `zpkg-format/sym-only-sidecar/source.zpkg` | ZPK **35**（当前 43）| 无任何测试读，落后 8 个 minor |
| 各 `expected.json` | 记 minor **33 / 34** | **全仓库无人读**，与二进制互不一致 |

PR-B 计划：给 4 个 zpkg fixture 各补一份 committed `.z42.toml`（`[project].pack` 决定 packed/indexed、
`--release` 决定 strip/sidecar）→ 让 `xtask build test` 能像 zbc 一样一键 regen（消除
`zpkg-format/README.md` 与 `version-bumping.md` 第 9 步里那个「暂需手工逐个重生」的 TODO——
手工步骤必被遗忘，正是本次根因 1）→ 刷新那 2 个陈旧二进制 → 把门扩到 zpkg → 处置
`expected.json`（无人读）与两个孤儿 fixture。
