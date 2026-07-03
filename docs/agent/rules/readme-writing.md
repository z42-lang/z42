# 根 README 写作规范（仓库门面 · 骨架 · 索引 · 边界）

> 仓库根 `README.md` 的编写规范。目标：**门面自足**（几分钟看懂 z42 是什么、能否跑起来、
> 接下来去哪）、**索引优先**（README 是导航枢纽，不是知识载体）、**只链接不复制**（实质内容
> 都在 book / workflow / rules，README 只做定位 + 索引）。
> 文档体系定位见 [doc-system.md](doc-system.md)；**目录级** README（`src/**/README.md` 六段制）
> 不归本文件管，见 [`code-organization.md`](../../../.claude/rules/code-organization.md)。

---

## 一、定位与角色

根 `README.md` 是文档体系里的**仓库门面 / 人的瘦入口**——与 `CLAUDE.md` / `AGENTS.md`（大模型的
瘦入口）对称的一层。它不承载知识，只做三件事：**定位**（z42 是什么、当前处于什么阶段）、
**上手**（最短可跑路径）、**索引**（把用户与开发者分流到 book / workflow / rules / roadmap）。

| 层 | 承载 | 位置 |
|----|------|------|
| **仓库门面**（本规范） | 定位 + 上手 + 快速索引 | 根 `README.md` |
| 知识库（深入层） | 系统"是什么" | `docs/book/` |
| 操作手册 | 具体命令 | `docs/workflow/` |
| 开发规范 | 怎么干活 | `docs/agent/rules/` + `.claude/rules/` |

**判定铁律**：任何要往 README 加的内容，先问「它属于 book / workflow / rules / roadmap 吗？」
→ 是，就写到那边，README 只留**一条索引行**。这条规则把"README 该放什么"变成机械可判，
杜绝它膨胀成第二份文档。

## 二、固定骨架（H2 顺序固定，AI 按段补充）

| # | 段 | 装什么 | 边界 |
|---|-----|--------|------|
| 1 | 标题 + 一句话定位 + 迭代状态提示 | z42 是什么、当前 pre-1.0 不稳定 | — |
| 2 | Why z42 | 价值主张（问题 → 方案表） | 不展开论证，一格一句 |
| 3 | Core Features | 能力要点列表 | 每条一句话，机制细节链 book |
| 4 | Quick Start | **最短**可跑路径（clone → install → run） | 完整步骤链 `docs/workflow/quickstart.md` |
| 5 | Documentation | "我想做 X → 读 Y" 索引表 | Y 指向具体页，**不复制**其内容（见 §四） |
| 6 | Repository Layout | 目录树 + 每行一句话 | 与目录 README 呼应，不抄其内容 |
| 7 | Implementation Status | roadmap 摘要（phase 状态表） | 详情链 `docs/roadmap.md` |
| 8 | License | 许可证 | — |

- 某段暂无内容可**整段省略**，但出现的段**顺序不得变、名字不得改**——门面骨架稳定，读者与 AI
  都靠它定位。
- 段与段之间用 `---` 分隔，与现有 README 风格一致。

## 三、内容边界（防漂移，双向纪律）

与 [doc-system.md 第四节](doc-system.md)「两层分工」同款，方向相反的两条都要守：

- README **不展开**设计原理 / 内部机制 / 完整命令清单 / 文件职责表——需要时**一句话 + 链接**。
- book / workflow **不反抄** README 的定位段与索引表——它们链接回 README，不复制。

**禁止反例**：

- ❌ 在 Core Features 里写某特性的实现算法 / 数据结构 → 那是 book 机制页的地盘
- ❌ 在 Quick Start 里堆全部构建/测试命令 → 那是 `docs/workflow/` 的地盘，README 只留最短路径
- ❌ Documentation 索引表里把目标页的内容摘抄进来 → 索引行只跳转，不复述

## 四、Documentation 索引段的组织

这是 README 的**核心价值**（"快速索引"）。用「我想做 X → 读 Y」表，**按受众分流**：

- **用户向**（理解设计 / 学语法 / 看特性 / 懂执行模型）→ 指向 `docs/book/` 对应部分概览页
- **开发向**（怎么构建测试 / 协作流程 / 看进度）→ 指向 `docs/workflow/`、`docs/agent/rules/`、
  `docs/roadmap.md`

每条 = `| 我想…… | [目标页](相对路径) |`，一行一意图，目标指向**可解析的具体页**，不指目录笼统带过。

## 五、维护触发表（改了什么 → 同步 README 哪段）

| 改了什么 | 同步 README 哪段 |
|---------|-----------------|
| 新增顶层特性 / 能力 | Core Features + Documentation 索引 |
| book 新增部分 / workflow 新增主题 | Documentation 索引加行 |
| 顶层目录结构变动 | Repository Layout |
| roadmap phase 状态推进 | Implementation Status（与 `roadmap.md` 保持一致） |
| Quick Start 涉及的命令变动 | Quick Start（只留最短路径，与 `workflow/quickstart.md` 对齐） |

改动落在上表任一行，**当次迭代内同步 README 对应段**，与 book/workflow 的同步同一触发点完成。

## 六、行文纪律

- **语种**：根 README 是**对外门面**，用**英文正文**（沿用 [doc-system.md 第八节](doc-system.md)：
  对外文档英文）。本规范文件本身是内部协作材料，故用中文。
- **只描述当前状态**：不写考古注记、不写"不再是什么"的历史对照（沿用
  [doc-system.md 第七节](doc-system.md)）。
- **结论先行、言简意赅**：门面给的是"够上手"的信息量，深度一律靠链接兑现。

## 七、与其他规则的关系

- 文档体系顶层地图与"这该写哪份文档"：[doc-system.md](doc-system.md)
- 目录级 README（六段制）：[`code-organization.md`](../../../.claude/rules/code-organization.md)（D8）
- book 页面写作：[book-writing.md](book-writing.md)
