# 并行开发：PR 隔离模型

> 触发条件：**任何一次向 main 落地的变更**（无论单 change 还是多 change 并行）。
> 流程主线见 [workflow.md](workflow.md)；本文件补齐其**分支 / PR / 并行**维度。
>
> 并行带来的唯一需要认清的代价见下「§4 语义耦合的兜底」。

---

## 核心模型（一句话）

**每个 change 一条独立 worktree + 独立分支（物理隔离，不共用分支 / worktree），完工开 PR；PR 按先来后到
合并，不排队。合并前每个 PR 必须并入 main 最新改动并重跑完整 GREEN；合并后立即删远程 + 本地
分支 + worktree。**

worktree 把并行流物理隔离，git 负责文本冲突，GREEN gate 负责语义正确。

> **§0 worktree 隔离铁律（必须遵守）**：**每一次改动都必须在自己专属的 worktree 里做，
> 一 change 一 worktree 一分支，绝不共用分支 / worktree、绝不在主树（`z42-test`）上直接改。** 无论改动
> 大小（feature / refactor / fix / 甚至纯文档规范）一律如此——主树只作 seed 供体与 origin/main 参照，不
> 承载在制品。理由：① 主树常被并发会话共享，在其上改动会互相踩踏；② 共用分支会让两条独立改动的历史 /
> GREEN 互相污染，无法按 PR 先来后到独立合并。新 worktree 必基于 origin/main（先 `git fetch`，别基于滞后
> 的本地 ref），供种（`.z42` / `xtask` / `xtask.zpkg`）从一个 warm 树拷贝后用种子 z42c 现建。
>
> **供种拷来的 `xtask` 必须立刻按当前源码重建**（`./artifacts/.z42/z42 publish scripts/xtask.z42.toml`，
> 先 `./xtask build all` 备齐 z42c/stdlib/launcher）。门禁逻辑本身就编在 `xtask.zpkg` 里，拿供体树那份
> 旧的去查本树的新文档/新测试 = **保证假红**。

---

## §1 分支 / 直推策略

| 改动规模 | 落地方式 |
|---------|---------|
| **很小的改动**（单行 fix / typo / 纯文档一处 / 显然无耦合的机械改） | 可直接 push main（走完整 GREEN 后） |
| **其余一切**（feature / refactor / 跨文件 fix / 任何 lang·ir·vm 变更） | **必走 PR**：开分支 → 实施 → GREEN → 开 PR → 合并 |

> 拿不准算不算"很小" → 按走 PR 处理。开 PR 的成本远低于直推 main 后发现要回滚。

**分支命名**：kebab-case、动词开头、≤ 5 词（见 [workflow.md「PR 即在制品账本」](workflow.md)），如
`add-for-loop`、`fix-type-check-crash`。**所有改动一律在专属 worktree 里开分支（§0 铁律），不在主树原地
开分支、不共用他人分支**——即便是"很小的改动"直推 main，也从自己的独立 worktree 走完整 GREEN 后再推。

### §1.1 PR body 约定（必须遵守）

PR 描述就是这次变更的方案与记录，**按变更模式分模板**——完整模式（lang / ir / vm）与最小模式（fix / refactor）的小节
见 [workflow.md 阶段 2](workflow.md)（模板的唯一 SoT，不在此复制）。无论哪种模式，都必须含：

- **What / Why**：一句话，本 PR 做什么、为什么
- **验证**：首行 `base: <本轮 GREEN 基于的 main sha>`（§3.1 强制：没有它，审阅者无从判断绿灯还算不算数），
  随后是 GREEN 状态——`xtask test` 全绿，或关键 stage 结果 / 对账证据（如自举字节不动点 gen1==gen2）
- 末尾页脚：`🤖 Generated with [Claude Code](https://claude.com/claude-code)`

- **标题**沿用 commit summary 格式 `type(scope): 描述`（见 [commit-log.md](commit-log.md)），与首个 / 主 commit 一致。
- **页脚必附**（与 commit 的 `Co-Authored-By` 对称）：格式见 [commit-log.md「页脚」](commit-log.md)。
- 多 commit 的 PR，body 的 What/Why 概述整条 PR，不复述每个 commit。

---

## §2 PR 合并顺序：先来后到，不排队

**多个 PR in-flight 时，谁先 GREEN + 就绪谁先合，无需声明占用。**

- 两个 PR 改**不同**子系统：天然无关，各自合。
- 两个 PR 改**同一**子系统、甚至同一文件：git 的文本冲突在 rebase（§3）时暴露；语义冲突由 GREEN 兜底（§4）。**不预先串行、不排队**——让先就绪的先合，后者 rebase 上去再跑。

---

## §3 合并前必须并入 main 最新改动（必须遵守）

**每个 PR 在合并前，必须先把 main 的最新改动并进来（rebase 或 merge main），并在并入后重跑完整
GREEN（`xtask test` 全 stage gate），全绿才能合。**

为什么强制：

1. **防版本落后**：分支开出去后 main 可能已合入其它 PR（尤其自举链的格式 / 种子 / stdlib API 变更），
   不并入就合 = 拿旧 main 的假设合进新 main，埋隐患。
2. **这是语义耦合的唯一兜底**（见 §4）——只有 rebase 到已合并的 PR 之上再跑 GREEN，
   两个同子系统 change 的语义冲突才会以**测试失败**的形式暴露在合并前。

**跳过 rebase-后-GREEN 直接合 = 违规**，等同于 workflow 阶段 5「未全绿即 commit」。

> **这条规则有一个（很薄的）自动兜底**：`test-host(linux-x64)`（required check）末尾会把
> **最新** main 并进来、重跑一次 `xtask check diagcodes`，所以**抢同一个诊断码号**这一种冲突
> 会在 CI 里红出来——见 [CI 拓扑 §3.0](../../internals/src/devinfra/ci.md)。它只覆盖码号这**一个**
> 维度、而且有窗口（你最后一次 CI 到按下 merge 之间 main 又前进了），**不能替代本节**：
> 语义冲突照旧只有「rebase 到最新 main + 重跑完整 GREEN」才暴露得出来。
> 按下 merge 前重跑一次那个 job，可以把码号那一维的窗口压到近零。

### §3.1 GREEN 结论会过期

**「绿了」不是一个属性，是一个**对某个 main sha 的**断言。main 一动，断言就可能失效，而绿灯
本身不会自动熄灭。** 所以：

- **合并前若 main 已前进到你跑 GREEN 时的 sha 之外，必须重跑再合**——不是「今天跑过了」，
  是「针对**这个** base 跑过了」。
- **PR body 里写明本轮 GREEN 基于 main 的哪个 sha**。PR 往往不是你本人、也不是当场按的合并键；
  没写 sha，审阅者无从判断那个绿灯还算不算数。

**验证—合并之间有窗口**：§3 的并入 + 重跑若做在**合并前几小时**，窗口里落地的 PR 会让那次 GREEN 的结论过期。

**本地 GREEN 可能被热产物掩盖**：夹具的构建产物是**热**的（缓存命中就不重编），
编译错误可能只在**冷构建**出现。

⇒ **本地 GREEN 的绿灯还隐含「产物状态」这个前提**。改动会影响编译产物时（编译器 / 清单解析 /
发现规则），合并前至少跑一轮**冷**的：

```bash
rm -rf src/compiler/*/dist src/toolchain/builder/tests/fixtures/z42b/*/artifacts   # 清掉会掩盖问题的热产物
xtask test
```

> 两者同一个形状：**绿灯的前提变了，绿灯却不会自己失效**。前提是 base sha，或是产物状态。

### §3.2 改「跑测试的工具」时，它依赖的源码修复必须**先**进 main

**判据：当一次变更让 gate 用**更严格的方式**编译/运行既有源码时，那些源码的配套修复必须是
**独立的、先落地的** PR —— 不能和工具变更放在同一个 PR 里。**

因为 A/B 类门禁会**拿新工具去跑旧树**。`bench-regression` 就有这么一步：

```
Capture base micro baseline (base tree, same runner)
```

它故意用 **PR 的 runner** 去跑 **base 树**，这样比较结果才不被 runner 变更污染。于是：
PR 里同时含「runner 改成按包编译」+「某源文件补 `using`」时，**base 树没有那条 `using`** ⇒
新 runner 一编就 E0436 ⇒ 基线 0 条 ⇒ 判红。PR 本身的实现没有任何问题。

**本地 GREEN 永远测不出这一类** —— 本地只跑当前树，不会拿新工具去跑旧树。

**别把「这次没红」当成可以合并提交的依据**——没有「用新 runner 跑旧树」步骤的路径只是侥幸没红。

---

## §4 语义耦合的兜底（必须认清）

并行 PR 要防的**不是 git 文本冲突**，而是**语义耦合返工**：

> 同子系统内"看似不重叠实则微妙耦合"是返工高发区（尤其 `runtime` 的 GC/JIT/safepoint 边界，
> 或共享基础设施文件如 `PackageCompiler` / `WorkspaceBuildOrchestrator`）。

worktree + PR 解决**物理隔离**和**文本冲突**，但**解决不了语义耦合**：两个 PR 改 runtime 的不同文件、
行不重叠、各自都能 clean merge，合在一起逻辑却可能坏——git 测不出来。

**兜底机制 = §3 的强制 rebase + 完整 GREEN**：后合并的 PR 必须 rebase 到已合并的那个之上、重跑全套
测试，语义冲突就会变成**红测试**挡在合并前。

**这是用「返工换并行度」的权衡，不是「冲突消失了」**：

- PR 隔离：放开并行度，把返工**推到合并前**（后者 rebase 时才发现要改）——在合并前重跑测试时暴露、就地修。

pre-1.0 快速迭代期这个权衡通常值：并行度更高，返工由 GREEN gate 自动兜住、不会静默进 main。
**但若某两个同子系统 change 明显深度耦合（如都在动 GC safepoint 语义），开工前主动在 PR 描述里
互相知会一声，能省掉一轮 rebase 返工**——这是建议，不是强制。

### §4.1 抢同一个「全局唯一编号」时，按**合并顺序**让号

有一类冲突 git 也测不出来：两个 PR 各自从一张登记表里挑了同一个**没人用的编号**（诊断码号最典型）。它不是语义耦合，也不总是文本冲突——各自都合法，
**合起来才错**。

**裁决规则：先来后到，tie-break 用「合并顺序」**——先合进 main 的那个保留编号，后合的让号。
不用「谁先想到」「谁引用得少」「谁改动小」之类的判断；那些每次都要重新吵一遍，而合并顺序是
git 里的客观事实。诊断码的完整流程（怎么占号、怎么让号、改号要全仓跟哪些地方）以
[错误码全表](../../reference/src/appendix/error-codes.md)的「新增一个码」一节为唯一 SoT，
**不要在别处复制那套规则**。

配套的两件事：占号必须落在**唯一的登记表**里（那样抢号会变成 git 文本冲突，而不是欢快合并），
以及 `test-host(linux-x64)` 末尾的抢号预检（见 §3 的注）。

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
  那些仍需单独确认，见 [workflow.md 阶段 3 批量授权边界](workflow.md)）。
- worktree 若有未提交改动，`git worktree remove` 会拒绝——先确认没漏东西再删。
- **合并后只做「清理」，不做「补内容」**——文档同步与方案定稿都是 PR **内**的事。
  完整论证见 [workflow.md 阶段 6 铁律](workflow.md)。

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
> ② §4 判断"两个 in-flight PR 是否同子系统、要不要互相知会"。

---

## 与其他规则的关系

- **workflow.md 阶段 2 / 6**：阶段 2 开工（worktree + 分支 + draft PR）；阶段 6 走 PR 合并 + §5 清理
  （小改直推 main 的例外见 §1）。
- **并行冲突**：代码与 markdown 一视同仁——文本冲突交给 git rebase、语义冲突交给 §4，不预先串行。
- **philosophy.md 根因修复**：某两个 change 反复语义打架 → 说明该子系统耦合过重，按根因修复评估拆分。
- **bootstrap-seed.md**：自举链（格式 / 种子 / stdlib API 两-nightly 纪律）的约束是
  跨 nightly 的发布周期约束，与分支并行是正交的两回事。
