# Tasks: 刷新 zbc golden 字节基线 + 加防腐门（refresh-format-fixtures）

> 状态：🟢 PR-A + PR-B 实施+验证完成 | 创建：2026-09-04 | 类型：fix + chore（不改产品行为 → 最小化模式）
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

## PR-B：zpkg 侧（同源，一并根治）

审计发现 zbc 之外更陈旧，且腐坏层数更多：

| # | 腐坏 | 处置 |
|---|---|---|
| 2 | `packed-multi-module` 停 ZPK **42**、`sym-only-sidecar` 停 **35**（落后 8 个 minor）| 全部 regen 到 43 |
| 3 | 上面两个 **无任何测试读** —— 所以才会烂得没人知道 | 新防腐测试覆盖全部 fixture |
| 4 | 4 个 `expected.json` **全仓库无人读**、记着 minor 33/34、与二进制互不一致 | 删除（README 表已含同等信息且更准） |
| 5 | `version-bumping.md`「版本常量**唯一真相表**」**自己就是错的**：写 zbc 1/35、zpkg 0/40（实际 1/38、0/43），路径也在 reader 拆分后失效 | 修正 + 标注它无测试兜底 |

> 第 5 条最说明问题：**连那份专门用来防止漏改的 checklist，自己都漏改了**。
> 所以本 change 的核心不是刷字节，而是让「陈旧」从静默变成红。

- [x] 3.1 4 个 fixture 各补 committed 构建配方 `<fixture>.z42.toml`
      （`[project].pack` 决定 packed/indexed；是否带 `--release` 决定 strip/sidecar）
      —— 消除 README 与 checklist 第 9 步那个「暂需手工逐个重生」的 TODO；手工步骤必被遗忘，正是根因 1
- [x] 3.2 **配方正确性强验证**：用新 toml regen `indexed-minimal` 得到**逐字节不变**的产物
      （它恰是唯一被真实加载测试消费的那个）。另三个如期变化 —— 因为 committed 那份是
      #414 手工 byte-patch 过 header 的旧构建，不是 writer 的真实输出
- [x] 3.3 4 个 zpkg 全部刷到 43；`indexed-minimal/source.zbc` 保持 38（已是当前）
- [x] 3.4 新增 `src/runtime/tests/format_fixture_versions.rs` —— **读 committed 字节**断言
      header 版本 == 当前 `ZBC/ZPKG_VERSION_*` 常量。zbc 与 zpkg 两套一起覆盖
- [x] 3.5 **反向验证该门真能抓陈旧**：把 `sym-only-sidecar` 退回 origin/main 的 0.35 → 测试立刻
      FAILED 并指名文件与版本；恢复后转绿。这道门若早存在，#414 当场就被拦下
- [x] 3.6 删 4 个 `expected.json`；修正 README 段落表（原写 `EXPT`，实际是 `IMPL`/`BLID`）+
      补完整 regen 命令 + 标注「覆盖不均」（只有 2 个 fixture 被功能测试消费）
- [x] 3.7 修正 version-bumping.md 的版本常量表 + 第 9 步补记配方与防腐门

### PR-B 验证

- [x] 4.1 `cargo test --test format_fixture_versions` 2 passed（+ 上述反向验证）
- [x] 4.2 `cargo test lazy_loader` 28 passed / `indexed_zpkg` 3 passed（消费方未受影响）
- [x] 4.3 `xtask test` 完整 GREEN（all stages passed；行数门 0 new/grown）

## Deferred

- **`expected.json` 的段结构断言**：删掉后，「每个 fixture 应含哪些 section」只剩 README 表这一处
  人读文档。若将来想让它可执行，可让 `format_fixture_versions.rs` 顺带解析 section 表并断言 ——
  需引入 JSON 依赖，本次未做。
- **两个 fixture 仍无功能测试消费**（`packed-multi-module` / `sym-only-sidecar`）：现由版本防腐门
  兜住不会再悄悄烂，但「它们的 wire layout 是否仍正确」没有测试回答。要么接进
  `lazy_loader_tests`，要么评估删除 —— 留作独立决策。
