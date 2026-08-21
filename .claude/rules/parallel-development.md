# 并行开发：PR 隔离模型

> 触发条件：**任何一次向 main 落地的变更**（无论单 change 还是多 change 并行）。
> 流程主线见 [workflow.md](workflow.md)；本文件补齐其**分支 / PR / 并行**维度。
>
> **历史（2026-08-01 迁移）**：本文件此前是「子系统互斥锁 + `docs/spec/changes/ACTIVE.md` 账本」。
> worktree 可完全物理隔离后，改为 **PR 隔离模型**——`ACTIVE.md` 账本与子系统锁**已废除**
> （文件已删）。废锁的权衡与兜底见下「§4 语义耦合的兜底」，这是这次迁移里**唯一需要认清的代价**。

---

## 核心模型（一句话）

**每个 change 一条独立 worktree + 独立分支（物理隔离，不共用分支 / worktree），完工开 PR；PR 按先来后到
合并，不排队、不占锁。合并前每个 PR 必须并入 main 最新改动并重跑完整 GREEN；合并后立即删远程 + 本地
分支 + worktree。**

worktree 把并行流物理隔离，git 负责文本冲突，GREEN gate 负责语义正确——三者合起来，锁不再需要。

> **§0 worktree 隔离铁律（2026-08-21 强化，必须遵守）**：**每一次改动都必须在自己专属的 worktree 里做，
> 一 change 一 worktree 一分支，绝不共用分支 / worktree、绝不在主树（`z42-test`）上直接改。** 无论改动
> 大小（feature / refactor / fix / 甚至纯文档规范）一律如此——主树只作 seed 供体与 origin/main 参照，不
> 承载在制品。理由：① 主树常被并发会话共享，在其上改动会互相踩踏；② 共用分支会让两条独立改动的历史 /
> GREEN 互相污染，无法按 PR 先来后到独立合并。新 worktree 必基于 origin/main（先 `git fetch`，别基于滞后
> 的本地 ref），供种（`.z42` / `xtask` / `xtask.zpkg`）从一个 warm 树拷贝后用种子 z42c 现建。

---

## §1 分支 / 直推策略

| 改动规模 | 落地方式 |
|---------|---------|
| **很小的改动**（单行 fix / typo / 纯文档一处 / 显然无耦合的机械改） | 可直接 push main（走完整 GREEN 后） |
| **其余一切**（feature / refactor / 跨文件 fix / 任何 lang·ir·vm 变更） | **必走 PR**：开分支 → 实施 → GREEN → 开 PR → 合并 |

> 拿不准算不算"很小" → 按走 PR 处理。开 PR 的成本远低于直推 main 后发现要回滚。

**分支命名**：沿用 change 名（`docs/spec/changes/<change-name>/` 的 kebab-case 名），如
`add-for-loop`、`fix-type-check-crash`。**所有改动一律在专属 worktree 里开分支（§0 铁律），不在主树原地
开分支、不共用他人分支**——即便是"很小的改动"直推 main，也从自己的独立 worktree 走完整 GREEN 后再推。

### §1.1 PR body 约定（必须遵守）

`gh pr create` 的 body 至少含以下三段，末尾附页脚：

```markdown
## What / Why
[一句话：本 PR 做什么、为什么]

## 验证
[GREEN 状态：`xtask test` 全绿，或列关键 stage 结果 / 对账证据（如自举字节不动点 gen1==gen2）]

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

- **标题**沿用 commit summary 格式 `type(scope): 描述`（见 [commit-log.md](commit-log.md)），与首个 / 主 commit 一致。
- **页脚 `🤖 Generated with [Claude Code](https://claude.com/claude-code)` 是 PR body 必附项**——与
  commit 的 `Co-Authored-By: Claude …` 页脚（[commit-log.md](commit-log.md)）对称：一个标记提交、一个标记 PR。
- 多 commit 的 PR，body 的 What/Why 概述整条 PR，不复述每个 commit。

---

## §2 PR 合并顺序：先来后到，不排队

**不再有子系统锁。多个 PR in-flight 时，谁先 GREEN + 就绪谁先合，无需查任何账本、无需声明占用。**

- 两个 PR 改**不同**子系统：天然无关，各自合。
- 两个 PR 改**同一**子系统、甚至同一文件：git 的文本冲突在 rebase（§3）时暴露；语义冲突由 GREEN 兜底（§4）。**不预先串行、不排队**——让先就绪的先合，后者 rebase 上去再跑。

---

## §3 合并前必须并入 main 最新改动（必须遵守）

**每个 PR 在合并前，必须先把 main 的最新改动并进来（rebase 或 merge main），并在并入后重跑完整
GREEN（`xtask test` 全 stage gate），全绿才能合。**

为什么强制：

1. **防版本落后**：分支开出去后 main 可能已合入其它 PR（尤其自举链的格式 / 种子 / stdlib API 变更），
   不并入就合 = 拿旧 main 的假设合进新 main，埋隐患。
2. **这是废锁后语义耦合的唯一兜底**（见 §4）——只有 rebase 到已合并的 PR 之上再跑 GREEN，
   两个同子系统 change 的语义冲突才会以**测试失败**的形式暴露在合并前。

**跳过 rebase-后-GREEN 直接合 = 违规**，等同于 workflow 阶段 8「未全绿即 commit」。

---

## §4 语义耦合的兜底（废锁的代价，必须认清）

废掉的子系统锁**原本防的不是 git 文本冲突**，而是**语义耦合返工**：

> 同子系统内"看似不重叠实则微妙耦合"是返工高发区（尤其 `runtime` 的 GC/JIT/safepoint 边界，
> 或共享基础设施文件如 `PackageCompiler` / `WorkspaceBuildOrchestrator`）。

worktree + PR 解决**物理隔离**和**文本冲突**，但**解决不了语义耦合**：两个 PR 改 runtime 的不同文件、
行不重叠、各自都能 clean merge，合在一起逻辑却可能坏——git 测不出来。

**兜底机制 = §3 的强制 rebase + 完整 GREEN**：后合并的 PR 必须 rebase 到已合并的那个之上、重跑全套
测试，语义冲突就会变成**红测试**挡在合并前。

**这是用「返工换并行度」的权衡，不是「冲突消失了」**：

- 锁：牺牲同子系统并行度，把返工**堵在开工前**（根本不让两个同子系统 change 同时进行）。
- PR 隔离：放开并行度，把返工**推到合并前**（后者 rebase 时才发现要改）。返工没消失，只是从"排队等"
  变成"合并前重跑测试时暴露、就地修"。

pre-1.0 快速迭代期这个权衡通常值：并行度更高，返工由 GREEN gate 自动兜住、不会静默进 main。
**但若某两个同子系统 change 明显深度耦合（如都在动 GC safepoint 语义），开工前主动在 PR 描述里
互相知会一声，能省掉一轮 rebase 返工**——这是建议，不是强制。

---

## §5 合并后清理（必须遵守）

**PR 合并后立即删除该 change 的远程分支 + 本地分支 + worktree**，不留残枝。

```bash
# PR 合并后（在 main 上）
git worktree remove <worktree-path>        # 若走了 worktree
git branch -d <branch>                      # 本地分支
git push origin --delete <branch>           # 远程分支
```

- 删自己这条已合并 PR 的分支 / worktree 属**默认授权**，无需再问 User（不同于 force-push / 删他人分支，
  那些仍需单独确认，见 [workflow.md 阶段 6.5 边界声明](workflow.md)）。
- worktree 若有未提交改动，`git worktree remove` 会拒绝——先确认没漏东西再删。

---

## 子系统划分（保留：供 commit scope 命名 + 语义耦合自查用）

| 子系统 | 范围 |
|--------|------|
| `compiler` | `src/compiler/`（z42c 自举编译器源码，用 z42 写） |
| `runtime` | `src/runtime/`（Rust VM：interp / jit / aot / gc） |
| `stdlib` | `src/libraries/`（.z42 标准库） |
| `toolchain` | `src/toolchain/` + xtask dispatch |
| `docs` | `docs/` |

> 这张表现在的用途：① [commit-log.md](commit-log.md) 的 `type(scope)` 里 scope 取值；
> ② §4 判断"两个 in-flight PR 是否同子系统、要不要互相知会"。**不再用于上锁**。

---

## 与其他规则的关系

- **workflow.md 阶段 2 / 9**：阶段 2 开分支（不再查账本占用）；阶段 9 归档后走 PR 合并 + §5 清理
  （不再"释放子系统锁"、不再直接 `git push origin main`，小改例外见 §1）。
- **workflow.md 阶段 3 冲突表**：docs/markdown 的段级冲突判定仍留在阶段 3；src 代码不再有子系统锁，
  文本冲突交给 git rebase、语义冲突交给 §4。
- **philosophy.md 根因修复**：某两个 change 反复语义打架 → 说明该子系统耦合过重，按根因修复评估拆分。
- **bootstrap-seed.md**：自举链（格式 / 种子 / stdlib API 两-nightly 纪律）的约束**不受本次废锁影响**——
  那是跨 nightly 的发布周期约束，与分支并行是正交的两回事。
