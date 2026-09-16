# Tasks: 开发规范收敛到 docs/agent/rules

> 状态：🟢 已完成 | 创建：2026-09-16 | 完成：2026-09-16 | 类型：docs（结构调整 + 消冲突）

**变更说明：** `.claude/rules/` 12 篇并入 `docs/agent/rules/`（16 篇归一），`.claude/CLAUDE.md`
退化为瘦入口；借这次搬迁消掉审计查出的 6 处矛盾、7 处重复全文、若干过时内容。

**原因：** User 2026-09-16 裁决「迁回 docs」。两处并存本身是冗余源，且
**批 0 立宪后产生了两条新冲突必须当场解决**（见下「我造成的」）。

**User 裁决：** ① 范围 = 搬迁 + 消冲突去重，**不动文件切分**（workflow.md 拆分等留待后续）；
② `docs/book/` 的祈使句**按 doc-system 三问分派**到 internals / reference。

## 阶段 1：机械搬迁（已完成）
- [x] 1.1 `git mv` 12 篇 → `docs/agent/rules/`；`.claude/rules/README.md` → `docs/agent/rules/README.md`
- [x] 1.2 删 `docs/agent/README.md`（agent/ 下只有 rules/ 一个子目录，一篇「本目录含 rules/」的
      README 是纯仪式；将来真有第二个子目录再加）
- [x] 1.3 **入向**引用重指 334 处 / 210 文件（深度感知脚本：6 种相对形态，各文件深度不同）
- [x] 1.4 **出向**链接重算 19 条（搬迁文件自身写的相对链接，深度从 2 变 3）
- [x] 1.5 死链门禁绿；基线 204 → **186**（重指顺带修好 18 条存量死链）

## 阶段 2：消冲突（批 0 造成的两条优先）
- [x] 2.1 🔴 **`docs/book/` 祈使句 ×12+**：批 0 冻结了 docs/book，但 `workflow.md` 仍要求
      「知识一律落 docs/book（唯一 SoT）」「新页挂入 docs/book/src/SUMMARY.md」⇒ 照做即违反冻结。
      按三问分派到 `docs/internals/`（机制）/ `docs/reference/`（规则）。
      同类：`readme-writing`(4) `code-organization`(2) `runtime-rust`(2) `common-pitfalls`(1) `learn-writing`(整表)
- [x] 2.2 🔴 **doc-system 节号引用全线失效**：批 0 把它重写成 8 节，8 处引用没跟
      （`readme-writing` 引「第四节」×3 +「第九节」**不存在**；`book-writing` 引「第四/六节」；
      `README` 引「第五节」；`CLAUDE.md` 引「§5.1」不存在）→ **改用节名/锚点，不用序号**
- [x] 2.3 GREEN 门禁组成：`workflow.md` 阶段 8 列 6 个 stage，实际 9+（漏 examples / docs / lines）
- [x] 2.4 pre-existing 失败：CLAUDE.md「含 pre-existing 都不得 commit」vs workflow「或单独 issue 跟踪」
      → 统一到 workflow 口径，CLAUDE.md 改为链接
- [x] 2.5 Scope 外根因：philosophy「请求 Scope 扩展」vs workflow「另开 change 不顺手修」
      → workflow 补一句判据（是本变更根因 → 扩 Scope；否则另开）
- [x] 2.6 阶段 9 的 `git add src/ docs/ examples/ .claude/` 全量暂存 vs commit-log「一个 commit =
      一个逻辑单元」→ 改为「add 本变更 Scope 内的路径」
- [x] 2.7 `README.md` 说 code-organization 管「5 段」模板 → 实为六段制
- [x] 2.8 philosophy 的 Deferred 目的地 `docs/design/`（已冻结）→ `docs/internals/` 机制页的
      `## Deferred` 节 + roadmap 索引行（**这条上次绕过去了，这次解决**）

## 阶段 3：去重（7 处重复全文）
- [x] 3.1 README 六段模板：`code-organization.md` 删内联，只留层级规则 + 链接 `readme-writing.md`
- [x] 3.2 触发矩阵 ×3：`code-organization.md` 删「README 同步规则」整节；`readme-writing.md` §10
      只留「同步哪一段」的段级信息
- [x] 3.3 「复杂逻辑写 book」：`CLAUDE.md` 压成一行 + 链 doc-system 三问第 2 问
- [x] 3.4 doc-check 清单 ×2：只留 doc-system（它是门③），workflow 改链接；把 workflow 独有的
      两条（死链 / 命令面 grep）并进 doc-system
- [x] 3.5 「归档随 PR」×3（且**互相声称详见对方，循环引用**）：完整版留 workflow 阶段 9，
      `parallel-development` §5 与 `commit-log` 各压一句
- [x] 3.6 PR 页脚 ×2：删 `parallel-development` §1.1 的说明，改链 commit-log
- [x] 3.7 「不写历史」×3（readme-writing 已是链接形式，无需改）：完整版留 doc-system §六.3，`book-writing` / `readme-writing` 改链接

## 阶段 4：清过时
- [x] 4.1 `CLAUDE.md`「实现计划」：焦点停在 0.3.x（roadmap 已把 REPL capstone 上移 0.4.0、反射标 ✅），
      且链的 `plan-0.3.x-three-streams/proposal.md` **是死链**（已归档）→ 压成一句「见 roadmap」
- [x] 4.2 `workflow.md` 阶段 8 的 `xtask test all` → `xtask test`（`grep '"all"'` 0 命中）
- [x] 4.3 `workflow.md` 删「`--scope`/`--parallel` 是 C# 版 xtask 的旧机制」（C# 已移除，属
      doc-system §六.3 禁止的「对已消失事物的对照」）
- [x] 4.4 `.claude/skills/{add-ir-op,next-phase}/SKILL.md`：引用指向冻结区，改指 internals
- [x] 4.5 `settings.json`：删 `Bash(dotnet *)`（C# 已移除）与三条指向**另一个 checkout 绝对路径**的条目
- [x] 4.6 `spec.md`（25 行）：管的两个文件都在冻结区 → 有效内容（示例代码块标注）并入 `book-writing`，
      「examples 不是特性示例库」与 `learn-writing` §四重复 → 删文件

## 阶段 5：入口与索引
- [x] 5.1 `docs/agent/rules/README.md` 重写为 16 篇的总入口（现标题还叫「.claude/rules/」），
      **补上一直漏列的 `readme-writing.md` / `book-writing.md`**（`git grep` 证实入口处不可达）
- [x] 5.2 `.claude/CLAUDE.md` 退化为瘦入口：项目简介 + 代码库结构 + 指路，实质规范全部链过去

## 阶段 6：验证
- [x] 6.1 `xtask test docs` 绿（含基线 --update 剔除已修好的）
- [x] 6.2 完整 `xtask test` 全绿

## 验证

**GREEN（基于 main 728ca9c8f）**：`xtask test` 全绿（6m30s，退出码 0）；`xtask test docs` 绿，
死链基线 **204 → 186**（重指顺带修好 18 条存量死链）。

## 备注

**不在本次做**（User 裁决「不动文件切分」）：`workflow.md`(806行) 拆四篇、
`commit-log` + `parallel-development` 合并、`code-organization` 拆二、按「何时读」重组分层。
审计已给出完整方案，留待后续 change。

**搬迁脚本的两个 bug（值得记）**：
① 规范名含数字（`compiler-z42c`）被 `[a-zA-Z-]+` 漏掉；
② **误改了 139 个 `docs/spec/archive/` 文件** —— 归档是历史记录，记录的是当时的事实（含当时的路径），
重写等于篡改留痕。已回滚并给脚本加 SKIP_PATHS。

**死链门禁抓不到的那类**：`../../README.md` 从旧位置指仓库根 README、从新位置指 `docs/README.md`，
**两个都存在** ⇒ 静默指错。所以出向链接必须**系统性重算**，不能只修门禁报出来的那几条。
