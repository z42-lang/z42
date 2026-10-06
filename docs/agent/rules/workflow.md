---
paths:
  - "**/*"
---

# z42 人机协作工作流（以 Pull Request 为单位）

> 不依赖任何外部工具：状态存在 git 与 GitHub PR 里。

---

## 核心模型

**一次迭代 = 一个 PR。PR 描述是这次变更的合同与记录**——开工前是方案（User 审批的对象），实施中是进度，
合并后是永久档案。不再有独立的「变更目录」：原来的 proposal / spec / design / tasks 合并成 PR 描述的几个小节。

```
Think First → Plan in PR → Build → Verify → Merge
  探索思考  → 方案写进 PR → 写代码 →  验证  →  合并
```

| 角色 | 职责 |
|------|------|
| **User** | 定方向、审批 PR 描述里的方案、裁决分歧 |
| **PR 描述** | 这次变更的人机合同，实现的唯一依据；合并后即档案 |
| **三本书** | 长期规范：用户能看见的规则落 `docs/reference/`，实现机制与决策落 `docs/internals/`（判据见 [doc-system 三问](doc-system.md)） |
| **Claude** | 自驱执行各阶段；不超 Scope；不猜测歧义 |

**User 介入点只有两个：** ① 方案审批（PR 描述）；② 方案分歧裁决。其余全部 AI 自驱。

**PR 是过程的家，不是知识的家。** 凡改变了对外行为或内部机制的，知识必须在**同一个 PR 内**上浮进三本书对应页；
PR 描述只记「这次为什么这么做、做了什么、验证了什么」。要查某特性为何如此：`git log -S<符号>` 找到提交，
提交标题带 `(#N)`，`gh pr view N` 看当时的方案与决策。

## PR 即在制品账本

| 要知道什么 | 看哪里 |
|-----------|-------|
| 现在有哪些变更在做 | `gh pr list`（含 draft）；别人的 PR 不碰 |
| 某个变更做到哪了 | 该 PR 描述里的任务清单（勾选框） |
| 某个变更方案是什么、User 批没批 | 该 PR 描述 + User 的确认（评论或对话中的明确肯定） |
| 过去某个变更为什么这么做 | `git log -S` → `(#N)` → PR 描述；长期结论已上浮到三本书 |

**命名**：分支用 kebab-case、动词开头、≤ 5 词（`add-for-loop`、`fix-type-check-crash`）；PR 标题用
`type(scope): 描述`（见 [commit-log.md](commit-log.md)）。

---

## 变更分类

| 类型 | 触发条件 | 模式 |
|------|---------|------|
| `lang` | 新语法、关键字、类型规则 | **完整模式**（阶段 0–7） |
| `ir` | 新 IR 指令、zbc 格式变更 | 完整模式 |
| `vm` | VM 执行语义变更 | 完整模式 |
| `fix` | Bug 修复，不改语义 | **最小模式**（PR 描述写变更说明 + 任务） |
| `refactor` | 纯重构，不改行为 | 最小模式 |
| `test/docs` | 测试或文档 | 直接实施，PR 描述写 What / Why 即可 |

**判断规则：类型优先，文件数作为 fix / refactor 内部细分。**
- `lang` / `ir` / `vm`：**无论文件数量**，一律完整模式
- `fix` / `refactor`：> 3 个文件 → 最小模式（带任务清单）；1–3 个文件 → 直接实施；单行 bugfix → 直接修改

**词汇警报**：检测到「新语法 / 新关键字 / 新 IR 指令 / 新约束 / 新接口契约 / 新类型规则 / 新 VM 行为」时，
**不论用户用什么语气**（即便说「快速开始」「直接做」），都走完整模式，不得降级。拿不准变更类型 → 按完整模式走：
多写几段 PR 描述的代价远小于跳过方案造成的返工和边界漂移。

## 🔴 Spec-First Self-Check（lang / ir / vm 强制）

**在写第一行实现代码之前，必须通过：**

```
[ ] 已开 draft PR，描述含 Why / What / Spec 场景 / Design / Scope / 任务清单（见阶段 2 模板）
[ ] User 已确认该描述（明确说「没问题 / 可以开始」，或在 PR 上批准）
[ ] 阶段 3「实施前确认」gate 已通过
```

**任一未达成 → 停，回到阶段 1–2 补齐，不得推进代码。**

**常见反例（皆为违规）**：
- ❌ 只有三本书里的对应页（长期规范）就开工 → 长期规范 ≠ 这次变更的方案，两者都要有
- ❌ 「迭代中和 User 逐步沟通过方案，所以 PR 描述从简」→ User 审批的是**写在 PR 描述里的文字**，不是聊天记录；
  对话里定下的内容必须写回描述
- ❌ lang / ir / vm 只写任务清单就开工（那是 fix / refactor 才允许的最小模式）
- ❌ 写完代码再补方案 → 方案的作用是在实施前定义边界，事后补齐只剩记录价值、失去约束价值

---

## 阶段 0：意图识别

**每次新对话首条消息触发：** 读取你所用工具的跨会话记忆（下文统称 memory：Claude Code 的 auto-memory，存于本机
`~/.claude/projects/<project>/memory/`，**不入库**；其他工具用其等价物）、当前阶段（`docs/roadmap.md`）和
进行中的 PR（`gh pr list`），主动汇报状态和下一步，再处理用户输入。

读到以下关键词时触发对应动作：

| 用户说 | 做 |
|--------|----|
| 「我想做 X」/「实现 Y」 | 阶段 1（探索） |
| 「继续」/「下一步」 | [会话恢复协议](#会话恢复协议) |
| 「直接做」/「快速开始」 | 最小模式（**仅 fix / refactor 适用**；词汇警报优先） |
| 「开始写代码」/「实施」 | 阶段 4（须先通过阶段 3） |
| 「没问题」/「可以开始」 | 阶段 3 通过 → 阶段 4 |
| 「批量确认」/「一并授权」/「这一系列都按计划做」/「自动推进」 | 阶段 3 批量授权模式 |
| 「停」/「暂停」/「不要继续」 | 终止批量授权，回到交互模式 |
| 「验证一下」 | 阶段 5 |
| 「合并」/「完成了」 | 阶段 6 |
| 「分析一下」/「探索」 | 阶段 1，不创建任何东西 |

## 阶段 1：探索（Explore）

输出，不创建 PR / 分支 / 文件：

1. 读取相关源文件，理解现有结构
2. 梳理核心问题 / 需求
3. 列出潜在风险和边界情况
4. 提出 2–3 个可行方案（如有），给出推荐及理由
5. **等待 User 选择方案**，再进阶段 2

**z42 专项检查：** 确认属于 `docs/roadmap.md` 的哪个阶段，未到的阶段特性拒绝推进；确认影响的 pipeline 组件
（Lexer / Parser / TypeChecker / Codegen / VM interp / JIT）；`gh pr list` 看有没有 in-flight 的 PR 改同一批文件。

## 阶段 2：开工——分支、worktree、PR 描述

1. **隔离**：按 [parallel-development.md §0](parallel-development.md) 在专属 worktree 里基于最新 `origin/main` 开同名分支。
   很小的改动可直推 main 的例外见该文件 §1。
2. **完整模式：先开 draft PR，再写代码。** PR 需要至少一个提交，用空提交承载（squash 合并时它会消失）：
   ```bash
   git commit --allow-empty -m "chore: 开工 <name>"
   git push -u origin <branch>
   gh pr create --draft --title "type(scope): 描述" --body-file <描述文件>
   ```
   **最小模式 / 直接实施**：实施完成、GREEN 后再开 PR（描述按下面对应模板）。
3. **把方案写进 PR 描述**（草稿先展示给 User，确认后写入；或先写入再请 User 审阅，均以描述为准）：

**完整模式模板（lang / ir / vm）：**

```markdown
## Why
[1–3 句：背景和问题，不做会怎样]

## What Changes
- [变更点列表]

## Spec（可验证场景）
### <Capability 名>
- **WHEN** <触发条件> **THEN** <预期结果>        # 正常场景
- **WHEN** <条件> **THEN** <结果>                  # 边界 / 异常场景
修改现有行为时写 **Before / After**。
lang / ir 类必含：**IR Mapping**（新语法对应的 IR 指令 / zbc opcode）+ **Pipeline Steps**
（受影响阶段勾选：Lexer / Parser·AST / TypeChecker / IR Codegen / VM interp）。

## Design
- **Architecture**：[ASCII 图或组件关系]
- **Decisions**：每条写「问题 / 选项 A·B 的优缺 / 决定与理由」
- **Testing Strategy**：单元测试 / golden test / VM 验证（`xtask test` 完整 GREEN）

## Scope（允许改动的文件）
| 文件路径 | 类型 | 说明 |
|---------|------|------|
| `src/path/Foo.z42` | NEW / MODIFY / DELETE / RENAME | … |
只读引用（理解上下文必须读但不改）：`src/path/Existing.z42` — 用于理解 X

## Out of Scope / Deferred
- [明确排除项；延后项另按 philosophy.md「延后特性管理」登记]

## Open Questions
- [ ] [待确认问题]

## 任务
- [ ] 1.1 [具体任务，指定文件和方法]
- [ ] 2.1 …（按 pipeline 顺序：Lexer → Parser/AST → TypeChecker → Codegen → VM interp → 测试 → 文档）

## 验证
base: <本轮 GREEN 基于的 main sha>
[阶段 5 的验证结论]
```

**最小模式模板（fix / refactor）：**

```markdown
## What / Why
**变更说明：** [一句话]　**原因：** [一句话]
**文档影响：** [需要更新的文档；无则写「无」]

## 任务                       # fix/refactor > 3 文件时必写；≤ 3 文件省略
- [ ] 1.1 [任务]
- [ ] 1.x 文档同步（若有行为 / 机制变更，按 doc-system 三问）

## 验证
base: <sha>
[GREEN 状态]
```

**Scope 表的硬性约束（完整模式必备，最小模式建议）：**
- 必须是项目内可解析的**具体文件路径**，不允许 `src/compiler/*` 通配，也不允许「相关测试文件」这种模糊描述
- 每条 NEW / MODIFY / DELETE 必须能被至少一项任务命中；反过来，任务触及的所有文件必须**全部**列入 Scope
- 实施中发现需要改 Scope 外文件 → **立即停下**，更新 PR 描述的 Scope，**User 重新确认**后才继续（批量授权下也是）

**并行冲突**：开工前 `gh pr list` 看是否有 in-flight PR 改同一文件 / 子系统。**不预先串行、不排队**：先就绪的先合，
文本冲突在 rebase 时暴露，语义冲突由合并前的完整 GREEN 兜底——见 [parallel-development.md §2–§4](parallel-development.md)。
明显深度耦合的（如都在动 GC safepoint 语义），在两边 PR 描述里互相知会。

## 阶段 3：实施前确认（Gate）

### 单 PR 模式（默认）

PR 描述就绪后，必须向 User 展示方案摘要并明确询问：

```
## 实施前确认

PR #N 的方案已就绪，请确认是否有问题：

- **Why / What：** [一句话]
- **Spec：** [场景数量] 个验证场景
- **Design：** [关键决策摘要]
- **Scope：** [N] 个文件
- **任务：** [N] 项

有问题请指出，没问题我开始实施。
```

- User 明确说「没问题」「可以」「开始」等肯定回复后，才能进阶段 4
- User 提出问题 → 修改 PR 描述 → 重新展示摘要 → 再次询问，**循环直到确认**
- **不得跳过**，即使 User 之前在对话里逐步确认过每一节
- 最小模式同样适用：展示描述摘要 → 确认 → 实施

### 批量授权模式（Batch Approval）

**触发**：一项规划被显式拆成多个 PR（如 C1 / C2 / C3），且 User 说「批量确认」「一并授权」「这一系列都按计划做」
「自动推进，不用每次问」等。

1. 一次性展示**所有受授权 PR 的摘要**（每个一段：Why / 场景数 / 关键决策 / 任务数）
2. User 明确说「全部开始 / 没问题 / 批量授权」→ 这些 PR 进入待实施队列，Claude 记录授权范围（PR 名单 + 确认时间）
3. 之后按依赖顺序逐个实施 → 验证 → 合并；每个合并后**自动开始下一个**，只汇报状态切换：
   ```
   ✅ C1 已合并（#N），1/4 完成
   🟡 现在开始 C2: <name>
   ```
4. 不再为每个 PR 单独展示摘要并询问；不再在每次 commit / push 后等待确认

**批量授权下仍然必须：** 每个 PR 独立通过 GREEN；每个 PR 单独成一个逻辑单元（不积压、不混合）；PR 内同步文档；
任何中断条件触发时立即停下。

**中断条件（必须停下询问，不得视为「已经全部授权」而自行决定）：**

1. **Scope 越界**：需要改授权 Scope 之外的文件 → 更新该 PR 的 Scope 或开新 PR
2. **测试失败超出当前 PR 范围**：pre-existing failure 或外部回归
3. **规范冲突**：两个 PR 的设计相互冲突，或与三本书现有规范冲突
4. **决策点未覆盖**：方案里没明确的设计点（字段命名、错误信息措辞、性能权衡），不得自行决定
5. **依赖前置变更需调整**：如 C1 落地后发现 C2 引用的 C1 字段需要重命名
6. **GREEN 失败**
7. **超出预期工作量**：实际任务量明显超出任务清单估计（如 1.5× 以上）
8. **架构性发现**：原本认为局部的变更其实牵涉跨模块重构

**边界：**
- **批量授权 ≠ 自动扩张授权**：范围严格限于一开始展示并被 User 确认的 PR 名单；新需求必须重新走「提议 + 单 PR 确认」
  或「扩展名单 + 重新确认」
- 对「代码实施 + commit + push + 开 PR / 合并 PR / 删自己这条已合并 PR 的分支·worktree」生效；对**其余外部影响动作**
  （force-push、删他人分支、改 CI 配置）仍需单独确认
- User 任何时候说「停」「暂停」「不要继续了」→ 立即终止批量授权

## 阶段 4：实施（Apply）

对每个任务：
1. 宣告「正在处理 N.M: [描述]」
2. 读取相关文件，实施代码变更
3. 把 PR 描述任务清单里对应项 `[ ]` → `[x]`（`gh pr edit --body-file`；允许在 push 时批量更新，不必逐项）
4. 简短说明完成情况；遇到阻塞 → 写进 PR 描述并告知 User

**任务粒度：** 每项对应一个明确的代码操作，30 分钟内可完成。

**z42 pipeline 顺序（不跳步）：** Lexer → Parser → AST → TypeChecker → Codegen → VM interp → 测试

**与方案偏差时：** 立刻停，不猜，不绕过。列出冲突 → User 裁决 → **先改 PR 描述**（决策变更、Scope 变更都写回去）→ 继续。已验证部分不回头重写。

**实施过程中的验证要求：**
- 每个 refactor / fix / feature 实施后，立即在本地运行编译 + 测试
- 测试失败：当前变更导致 → 立即修复；pre-existing 失败 → 本迭代修复，或说明原因后 User 确认；与当前 Scope 无关 →
  记入 PR 描述「Out of Scope」并另开 PR
- 不得跳过任何测试失败继续下一个任务；整个阶段 4 完成后，进阶段 5 前必须全绿

## 阶段 5：验证（Verify）

**全绿（GREEN）标准**——进阶段 6 前必须通过全部验证，且**所有测试全部通过**。统一入口：

```bash
xtask test          # 默认串联所有必跑 stage（完整 GREEN gate）
```

**iteration 期加速**：dev 期可用 `xtask test changed`（按改动文件挑 stage）或单跑某 stage（`xtask test e2e --dir/--file` /
`test stdlib <lib>` / `--no-build` 跳过重建波）缩窄。但 **commit 前最终 GREEN 必须跑完整 `xtask test`**——
partial 验证只算 dev 期快速 iterate，不替代门禁。缩窄手段只有这三种：`test changed` / 单 stage / `--no-build`。

裸 `test` 先跑 **regen 构建波**（stdlib + z42c 自建 + golden `.zbc` 基线 + debug VM；`--no-build` 可跳过），
随后按顺序跑全部 stage（任一失败立刻停）。

> **stage 清单不在这里复列。** 唯一 SoT 是 `_gateStageNames()`（`scripts/test/xtask_test.z42`）与 test-gate 文档的
> `gate-stages` 区，两者由门禁逐项对账、不一致即红。**复列必漂**，故不复列。

常用的单 stage（调试期缩窄用，**不替代**完整 `xtask test`）：

```bash
xtask build runtime                 # z42vm（Rust VM）
xtask test e2e                      # VM goldens（interp）
xtask test e2e --dir cross-zpkg     # 跨 zpkg 端到端
xtask test stdlib                   # stdlib [Test] dogfood
xtask test compiler                 # z42c 自举字节不动点
xtask test docs                     # 文档死链
xtask test docs examples                 # 学习手册 ↔ examples 重放
```

> **不要漏跑 cross-zpkg / lib / compiler**：它们不在默认 GREEN 路径之外，漏跑会让对应层的回归长期不被发现。
> 编译器正确性由 z42c 自举 stage 保证。

发行版变更（xtask package / 跨平台 / 嵌入接口）追加跑 `xtask test package`（先 `xtask package sdk` 产 host-RID 包）。

**测试失败处理：**

| 情况 | 处理方式 |
|------|---------|
| 当前变更导致的新失败 | ❌ 必须在本迭代修复，不得 commit |
| 当前变更触发的隐藏 bug | ❌ 必须修复，或明确说明理由后 User 确认 |
| Pre-existing 失败（变更前已存在） | ⚠️ 必须在**同一迭代**修复，或单独 issue 跟踪 |
| 问题与当前 Scope 无关 | ✅ 记入 PR 描述 Out of Scope，另开 PR，不阻塞本迭代 |

**验证报告（写进 PR 描述的「验证」段，并在对话中输出）：**

```markdown
## 验证
base: <本轮 GREEN 基于的 main sha>
xtask test：✅ 全绿（N stages）/ ❌ 失败 at <stage>      # 全绿时一行即可，失败才逐 stage 展开
Spec 覆盖：| 场景 | 实现位置 | 验证方式 | ✅ |             # 完整模式
任务：N/N ✅
结论：✅ 可合并 / ❌ 未全绿，待修复：[列出]
```

**全绿才能进阶段 6。** 未全绿不得 commit / push（草稿 PR 上的过程提交除外，但不得标 ready、不得合并）。

## 阶段 6：合并（Merge）

> **铁律：文档同步与方案定稿都在 PR 内完成，合并后只做清理，不补内容。** 禁止「代码先合、文档后补」，
> 禁止合并后再单独推一个文档提交到 main。

1. **文档同步**：判据是 **[doc-system.md 的三问](doc-system.md)**（唯一 SoT，此处不复列），逐问过一遍，命中的全部落实：
   1. 用户能看见吗？→ `docs/reference/` 对应页必改；已被 learn 覆盖 → `examples/<章节>/` + learn 章一并改
   2. 下一个接手的人不读文档能看懂吗？→ `docs/internals/` 对应机制页必改
   3. 目录的结构、对外入口或依赖变了吗？→ 该目录 `README.md` 必改（写法见 [readme-writing.md](readme-writing.md)）

   三问全否 = 纯内部重构，不补文档——**也不许顺手改文档制造漂移**。另有三处与三问正交、命中就得改：根 `README.md`
   （影响仓库门面时）、`docs/roadmap.md`（延后项被消化 / 新增，或阶段进度变化时）、`docs/agent/rules/`（改的是协作规则 /
   流程本身时）。**合并前 doc-check 清单**见 [doc-system.md「三道门」](doc-system.md)的门③，不在此复列。
2. **PR 描述定稿**：把描述更新成**最终状态**——实施中偏离方案的决策、实际 Scope、产生的 Deferred 项、验证结论。
   合并后它就是档案，不再改写。
3. **提交**（无需 User 确认）：`git add <本变更 Scope 内的路径>`（不是 `git add -A`），`git commit -m "type(scope): 描述"`；
   每个逻辑单元单独提交。
4. **落地到 main**——策略见 [parallel-development.md](parallel-development.md)：
   ```bash
   git push origin <branch>
   gh pr ready                              # draft → ready
   git fetch origin && git rebase origin/main
   xtask test                               # 并入最新 main 后必须全绿才合；base sha 写进描述
   gh pr merge --squash --delete-branch     # squash：main 上一个 PR 一个提交
   ```
   **squash 提交正文**写 Why + What 摘要（≤ 10 行）和关键决策；完整方案留在 PR 描述，靠提交标题的 `(#N)` 找回。
5. **合并后清理（必做，默认授权）**：删本地分支 + worktree（远程分支由 `--delete-branch` 删）。force-push / 删他人分支仍需单独确认。
6. **提供续推口令（必做）**：任何任务合并或提交落地后，若还有后续任务（同一程序的下一 PR、Deferred 项、阶梯下一阶段等），
   必须主动给出一个「续推口令」，让 User 可以清理上下文（`/clear`）以减少 token 消耗，之后凭口令在新会话无缝续推。具体动作：
   1. **把后续任务写进 memory**（`memory/<slug>.md`）——需求说明与前因后果必须写透，至少覆盖：**需求 / 目标**（解决什么问题、
      User 原话意图）；**前因后果 / 背景**（属哪个大程序 / Deferred、已合并的 PR 各自解决了什么、已裁决的关键决策及理由）；
      **关键机制入口**（file:line 与作用，别让下个会话重新探索已知事实）；**未决设计分叉**（须 User 裁决的点 + 备选 + 推荐）；
      **恢复环境**（worktree 路径 / 种子 / 构建·验证配方 / 踩过的坑）；**流程**（是否需完整模式、拆分建议、下一步第一个动作）。
      判据：**一个对本程序零记忆的新会话，只读这份 memory（+ 它链接的 PR），就能理解全貌并正确接手**。
   2. **定义口令**：一句简短无歧义的中文触发语（如「推进 struct P3」），在 memory 文件 `description` 和 `MEMORY.md`
      置顶指针行标注「🔑 口令『…』→ 读本文件续推」。
   3. **回复 User 时明确给出口令**，并附一句当前落地状态。没有后续任务时无需口令。

---

## 会话恢复协议

User 说「继续」时，自动执行：

1. `gh pr list --author @me`（含 draft）与当前分支对应的 PR（`gh pr view`）
2. 读该 PR 描述的任务清单，找到第一个未勾选任务
3. 读 [docs/agent/README.md](../README.md) → 规范入口与当前阶段约束；读 memory → 跨会话决策
4. 汇报：

```
当前变更：#N <name>
已完成：X/N 项
下一步：任务 N.M — [描述]
继续？
```

5. User 确认后继续

## 越界防护

**必须停下询问（不得擅自决定）：**
- 需要改动 Scope 外的文件（**即使在批量授权模式下也必须停**）
- 发现 Scope 外的 Bug → 记入 PR 描述 Out of Scope，另开 PR，不顺手修
  - **例外**：若它就是本变更要解决的问题的**根因**，按 [philosophy.md「系统性修复 vs Scope 控制」](philosophy.md)
    停下报告 + 请求 Scope 扩展，User 批准后从根因修——别在症状层打补丁
- 方案未覆盖的接口 / 架构选择；Done Condition 不明确
- 批量授权模式下任一中断条件触发

**自主决定（无需询问）：** 算法细节、数据结构选择、代码风格、变量命名。

**Scope 表的强约束：** PR 描述的 Scope 表是实施时**唯一允许触及的文件清单**。需改 Scope 外文件 → 立即停下更新 Scope，
更新后必须 User 重新确认（批量授权下也是）。「顺手改一下」「反正在附近」「应该没人在意」都是违规，无例外。

## 实现哲学 / 设计完整性 / 延后管理

以下规则不属于流程主线，独立沉淀在 [philosophy.md](philosophy.md)：实现方案原则（优先最终方案 / 不为破坏性顾虑而牺牲 /
修复必须从根因出发 / 不为旧版本提供兼容）、设计完整性原则（设计无法承载需求时停下讨论，禁止打补丁）、规范冲突检测与事实校正、
延后特性管理。`.zbc` / `.zpkg` 格式 version bump 的同步 checklist 见 [version-bumping.md](version-bumping.md)。

---

## 测试要求（必须遵守）

**每次新增需求或迭代（非纯文档 / 纯注释变更），必须包含对应的测试用例。无测试 = 未完成。**

| 变更类型 | 测试要求 |
|---------|---------|
| 新功能 / 新 pipeline 阶段 | 至少 1 个正常用例 + 1 个边界 / 异常用例的单元测试或 golden test |
| Bug fix | 至少 1 个回归测试，覆盖修复的 bug 场景 |
| 新 IR 指令 / VM 行为 | golden test (run/) 验证端到端执行结果 |
| 新 CLI 命令 / 工程文件字段 | 单元测试验证解析正确性 + 错误输入报错 |
| refactor | 确保已有测试仍覆盖（不必新增，但不得删除测试） |

**测试位置：**
- z42c 编译器：`src/compiler/<member>/tests/<name>/`（如 `z42c.semantics/tests/`、`z42c.syntax/tests/`）
- Rust VM：`src/tests/<category>/<name>/`（VM e2e）或 `src/runtime/src/*_tests.rs`（Rust 单测）；stdlib-bound 用例放 `src/libraries/<lib>/tests/<name>/`
- 跨语言端到端：golden test（source.z42 + expected_output.txt）

---

## 禁止行为

**必须遵守，违反即为严重工作流缺陷：**

- **方案未经 User 确认前写实现代码**：lang / ir / vm 变更必须先有 PR 描述（Why / Spec / Design / Scope / 任务）并经 User 批准；
  参见上文 **🔴 Spec-First Self-Check**。把聊天中的逐步确认当作方案审批也算违规——审批对象是 PR 描述
- **验证未全绿时 commit / push 到 ready 的 PR**：任何测试失败都不得进入合并；包括 pre-existing 失败（修复，或另开 issue + 说明）；
  验证命令必须完整运行：`cargo build && xtask test`
- **顺手修复 Scope 外问题**：Scope 内改动优先完成 + 验证；外部问题记入 Out of Scope，另开 PR
- **用「我理解你的意思是…」绕过歧义**：存在歧义时停下来询问，而不是主观推断
- **interp 未全绿时填 JIT / AOT 实现**：VM 实现严格按顺序 interp ✅ → JIT → AOT
- **跨阶段混入**：检查 `docs/roadmap.md`，确认当前阶段限制，后续阶段的特性不得混入
- **单个 PR 积压多个逻辑单元**：一个 PR = 一个逻辑单元；拆分（refactor）与功能变更分开成 PR
- **合并后补内容**：文档同步、方案定稿类改动不得在合并后另推到 main
- **未经 User 确认擅自采用临时方案**：存在「临时」与「最终」方案时默认实施最终方案；条件不允许（依赖未就绪、工作量超出 Scope）
  必须先向 User 说明并得到确认
- **批量授权下扩张授权范围**：授权对象是**当时展示的 PR 名单**，不是「所有相关工作」；实施中发现需要新增 PR、改动 Scope 外文件、
  调整既定决策 → 必须停下询问
- **Scope 表使用模糊描述**：❌「src/compiler/* 相关测试」「涉及的辅助函数」「对应的文档」；✅ 具体文件路径，每条对应至少一个任务，
  与任务清单双向对齐
