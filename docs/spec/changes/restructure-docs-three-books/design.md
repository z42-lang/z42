# Design: 文档三书重构

## 一、为什么是三本书：受众切分

现行体系把 `docs/book/` 定义为「用户 + 维护者 + 大模型」**共用**。这一条决定衍生出当前的全部病症：

- 实现细节在 book 里**没有一等位置** ⇒ `docs/design/` 的 99 篇无处可去 ⇒ 实测迁移速率 **81 篇迁了 2 篇（2.5%），历时两个半月**，而 `design/runtime/zbc.md` 还在被更新（book 对应页停在 07-19）——两份并行漂移的活证据。
- 语种策略无法收敛（§8：双受众故中文，成熟章节「再评估」英文）。
- `docs/workflow/packaging.md` 与 `docs/book/src/dev/packaging.md` 文件名直接撞车。

**不是执行力问题，是结构上没给实现细节留位置。** 按受众切开，每本书的深度、语种、是否发布都自洽。

> **旁证**：`book-writing.md` 早已定义了三种页型——**参考页**（逐项查询）/ **机制页**（为什么这样设计 + 怎么运作）/ **概览页**。这套分类本身就预示了本次拆分：参考页天然属 reference，机制页天然属 internals，概览页是每本书各部分的入口。拆书不是新发明，是把已有的页型分类**兑现成目录**。

## 二、三本书的角色（唯一判据）

判据必须能一句话判定；**右边那列同样重要**——冗余都是从「顺手也写一点」开始的。

| | `docs/learn/` 学习手册 | `docs/reference/` 语言与库参考 | `docs/internals/` 实现内幕 |
|---|---|---|---|
| **受众** | 用 z42 写程序的人（**学**） | 用 z42 写程序的人（**查**） | 改 z42 本身的人 / AI |
| **判据** | 按学习顺序读一遍就会用 | **不读实现也能用对** | **要动这块代码才需要读** |
| **回答** | 怎么上手做成一件事 | 规则是什么、有哪些、怎么写 | 怎么实现的、为什么这样、在哪改 |
| **组织** | 线性，后一章只依赖前面讲过的 | 按主题，可跳读，规则完整 | 按子系统，按机制 |
| **页型** | 章 | 参考页为主 + 少量概览页 | 机制页为主 + 概览页 + 操作页 |
| **代码** | 全部来自 `examples/`，门禁逐条重放 | 片段，说明用法 | 片段 + 伪代码 + mermaid，说明机制 |
| **明确不写** | 完整规则（链 reference）；任何实现细节 | 教学铺垫；实现机制；设计理由 | 用户用法（链 reference）；目录文件清单（链 README） |
| **语种** | 中文先行 → 出英文版 | 中文 → 出英文版 | 中文（内部） |
| **发布** | `/z42/learn/` | `/z42/reference/` | `/z42/internals/` |

配套（**不是书**，各有其职）：

| 位置 | 职责 | 不写什么 |
|---|---|---|
| `docs/agent/rules/` | 怎么干活的**行为约束** | **任何系统知识**——那是三本书的事 |
| `docs/spec/changes` + `archive/` | 这一次迭代在做什么 | 长期知识（归档时上浮到三书） |
| `src/**/README.md` | 这个目录有什么、怎么改 | 设计原理（链 internals） |
| `docs/roadmap.md` | 项目计划与 Deferred 索引 | 知识 |
| 根 `README.md` | 仓库门面与分流 | 实质内容 |

**链接方向（单向，硬规则）**：`learn → reference → internals`。

> **`reference` 不得链 internals。** 一旦反向链，用户就会被带进实现细节，受众混淆会从头开始——这正是现在 book 的病。internals 可以链任何一边。learn 链 reference 查规则，不直接链 internals。

## 三、内容规划

### 3.1 `docs/reference/` —— 语言与库参考

```
reference/src/
├── README.md                      前言：这本书是什么、怎么查
├── language/    语法规则（约 20 页）
│     README 概览 · 词法与源文件 · 类型系统 · 表达式与运算符 · 语句与控制流 ·
│     函数 · 类与对象 · 继承与接口 · struct 与 record · 枚举与模式匹配 ·
│     泛型（类型/方法/约束/Self/关联类型）· lambda 与委托 · 异常 ·
│     命名空间与访问控制 · partial · attribute 与反射 · 集合字面量 · 元组 ·
│     成员转发 · 可用性探测
├── stdlib/      逐包 API（约 25 页）
│     README 包索引与三层架构 + 每包一页
├── toolchain/   用户会敲的东西（约 7 页）
│     z42 命令参考 · z42.toml 清单字段 · 运行时设置（旋钮表）·
│     发布与部署 · workload 与平台 · 编辑器集成 · REPL
├── embedding/   在 C / Rust 宿主里嵌入 z42（C ABI 契约）
└── appendix/    错误码全量表 · 写给 C# 开发者 · 词汇表
```

> **`embedding/` 属 reference（User 2026-09-16 裁决）**：宿主开发者**也在「用 z42」**，只是从宿主侧用；
> C ABI 是**对外契约**而非内部机制，且 learn 第 34 章（在 C/Rust 程序中嵌入 z42）需要它。
> VM 内部如何实现这套 ABI 仍归 `internals/runtime/`。

**边界裁决（容易混的，写死在这里）**：

| 主题 | reference 写 | internals 写 |
|---|---|---|
| 错误码 | **全量码表**（码 → 含义 → 触发示例） | 码怎么分配、如何新增一个码、诊断如何产出 |
| zbc / zpkg | **不写**（用户不需要知道产物格式） | 完整格式规格 |
| 运行时设置 | 旋钮**清单**与取值语义 | 五层优先级如何实现、旋钮登记表机制 |
| 泛型 | `where` 能写什么、报什么错 | 约束检查器、TSIG、单态化策略 |
| GC | **不写**（除非有用户可感知的旋钮 → 归运行时设置） | 全部 |
| 测试 | `[Test]` 怎么写、`z42 test` 怎么用 | 测试流水线两层模型、GREEN gate 组成 |

### 3.2 `docs/internals/` —— 实现内幕

```
internals/src/
├── README.md                      前言 + 系统总览（三个进程怎么协作）
├── compiler/    z42c（约 20 页）架构 · 源码编译流程 · 符号与类型检查 · codegen ·
│               工程模型 · 工作区构建 · 增量缓存 · 自举与种子 · 诊断产出 · 错误码体系
├── runtime/     z42vm（约 30 页）执行模型 · 解释器 · JIT · GC（多页）· 对象布局 ·
│               加载上下文 · 诊断与 profiler · PAL 与跨平台 · native 扩展 · 嵌入
├── formats/     zbc · zpkg · 清单 schema —— 跨编译器与运行时的**协议**，独立成部分才好找
├── stdlib/      组织原则 · API 准则 · 平台分层 · 关键实现（json serde 等）
├── toolchain/   launcher · z42b 构建编排 · workload 与平台发布 · 测试流水线 · 打包引擎
└── dev/         开发操作（← `docs/workflow/` 整体并入）
                构建 · 测试门禁 · CI 拓扑 · 发布流程 · 调试配方 · 基准与 bench gate
```

> **为什么 `formats/` 独立成部分**：zbc / zpkg 是**编译器与运行时之间的协议**，两边都要查；挂在任一侧都会让另一侧的人找不到。且 strict-pin 的版本纪律是跨两者的约束。

> **为什么 `docs/workflow/` 并入 internals 而不是保留**：它的受众与 internals 完全相同（改 z42 的人），且现在与 `book/src/dev/` 文件名撞车、CI job 表三处并存。并入后「机制」与「怎么跑」在同一本书里互链，不再需要跨目录猜。

### 3.3 `docs/learn/` —— 学习手册

**目录结构不变**（第 1–3 章已上线，门禁已建立）。本次只做两件事：
1. 把书中链接 `docs/book/` 的地方改指 `docs/reference/`；
2. `OUTLINE.md` 的「深入实现」类链接改指 `docs/internals/`（仅限确实需要的少数几处）。

## 四、迭代规范

### 4.1 什么代码改动要补什么文档 —— 三问取代抽象矩阵

现行触发矩阵的行是「新增/删除文件」「对外行为变更」这类**抽象描述**，判定全靠人理解，且**有三份互不一致的拷贝**（`workflow.md` 阶段 9、`readme-writing.md` §5/§10、`code-organization.md`）。收敛为一份，并改用三问组织：

1. **用户能看见吗？**（语法 / stdlib API / CLI / 清单字段 / 诊断文本 / 产物格式）
   → `reference` 对应页必改；该特性若已被 learn 覆盖 → `examples/<章节>/` + learn 章一并改
2. **下一个接手的人不读文档能看懂吗？**（多阶段编排 / 有状态或累积的循环 / 反直觉决策与踩过的坑 / 跨组件或跨进程协议）
   → `internals` 对应机制页必改
3. **目录的结构、对外入口或依赖变了吗？**
   → 该目录 `README.md` 必改

**三问全否 = 纯内部重构，不补文档**——也不许顺手改文档制造漂移。

拿不准第 2 问算不算「复杂」→ **停下问 User**，别默默略过（沿用 doc-system §5.1）。

### 4.2 每本书各自的迭代节奏

| 书 | 何时更新 | 完备性靠什么保证 |
|---|---|---|
| `learn` | 按 `OUTLINE.md` 推进；被覆盖特性变更时同步 | `xtask test examples`：代码与终端输出**逐条真实重放**（已建立） |
| `reference` | 特性落地即更新规则页；新 stdlib API 即更新包页 | 人工 + 未来可机械对账（stdlib 导出面 ↔ 包页条目） |
| `internals` | change 归档时**知识上浮**；踩坑即补「为什么」 | 人工 + 页头「对齐」日期 |

### 4.3 三道门

| 门 | 内容 | 状态 |
|---|---|---|
| **① 同一个 PR** | 文档与代码同分支同 PR，禁止「代码先合、文档后补」 | 已有铁律，保留 |
| **②a 死链检查** | 相对链接可解析（`xtask test docs --links`） | **批 0 就落**（裁决 5）——搬 200 篇的全程都要有网 |
| **②b 其余门禁** | 每个 `.md` 挂进所属书 SUMMARY / 页头有「对齐」字段 / 命令面改名后旧名 grep 清零 / learn↔examples（已有 B1–B10）；并把 `docs/**` 从 `ci.yml` 的 `paths-ignore` 放出来 | 批 7（裁决 5） |
| **③ 归档 doc-check 清单** | 人工兜底，收敛为一份 | 已有，随总纲重写 |

> **User 裁决：不引入页头「跟踪」字段**（曾提议 `**跟踪**: <代码路径>` 做页↔代码的分布式双向校验）。保持中心化矩阵 + 三问判据。**别再提这个方案。**

### 4.4 防漂移三条硬规则

1. **SoT 铁律**：一条事实只有一个权威位置，其余只能链接。发现两处各写一份 → 按规范冲突检测停下。
2. **单向链接**：`learn → reference → internals`，reference **不得**反向链 internals（见第二节）。
3. **不写历史**：文档只描述当前状态；考古注记、迁移状态表、「取代旧方案」这类过程叙述一律不进正文（沿用现行 §7 行文纪律）。

**落点**：以上全部写进重写后的 `docs/agent/rules/doc-system.md`（唯一总纲）；`workflow.md` 阶段 9 与 `code-organization.md` 里的两份矩阵拷贝删除、改为链接。

## 五、迁移执行计划（分批 PR）

**批 0 必须先落**，它把后续每一批都变成机械搬运：

| 批 | 内容 | 规模 |
|---|---|---|
| **0 · 立宪** | 重写 `doc-system.md`（三书判据 + 三问 + 三道门 + 链接方向）；建 `reference/` `internals/` 骨架（`book.toml` + `SUMMARY` 骨架 + 各部分 README）；`deploy-book.yml` 扩到三书。**不搬任何内容** | 小 |
| **1 · internals/compiler** | `book/src/compiler`(12) + `design/compiler`(9) | 中 |
| **2 · internals/runtime + formats** | `book/src/runtime`(21) + `design/runtime`(25)，zbc/zpkg 切入 formats | 大 |
| **3 · reference/language** | `book/src/language`(22) + `design/language`(28)；**book 的空占位由 design 填上** | 最大 |
| **4 · reference/stdlib** | `book/src/stdlib`(4) + `design/stdlib`(22) | 中 |
| **5 · toolchain + dev** | `book/src/{toolchain,dev}`(12) + `design/{toolchain,testing}`(13) + `docs/workflow/`(25) | 大 |
| **6 · 收尾** | 删 `design/` 与 `workflow/` 空壳；重写 `docs/README.md`；游离文件处置；全仓链接重指 | 中 |
| **7 · 门禁** | `xtask test docs` 其余各项（SUMMARY 完整性 / 页头对齐 / 命令面 grep） | 中 |

**批 0 含最小死链检查**（User 裁决 5）：只查「相对链接可解析」一条，`xtask test docs --links`。
理由见 §6.1——批 1–6 是搬 200 篇文档的过程，全程没有网是本次最大的机械风险。

每批一个 PR，各自跑完整 GREEN。批 1–5 之间无依赖，可调序。

**游离文件处置**（批 6）：

| 文件 | 处置 |
|---|---|
| `docs/features.md`(546 行) | 语言设计决策 + phase 归属 → `internals/`（触发矩阵原指向它的那行改指 reference/internals） |
| `docs/library_review.md`(183 行) | **删**——2026-08-30 的一次性 stdlib 对标快照，不属目标结构任何一类，git 留痕 |
| `docs/todo-list.md`(12 行) | **删**——无结构散记，与 roadmap 的「当前焦点 / Deferred」重叠 |
| `docs/README.md`(75 行) | **重写**——现数字全错（说 design/runtime 10 篇实为 25）、缺 design/toolchain、缺 docs/learn |

## 五之二、User 裁决汇总（2026-09-16，八条）

| # | 裁决 | 影响 |
|---|---|---|
| 1 | **接受三本书** learn / reference / internals | 全局 |
| 2 | **internals 对外发布**（`/z42/internals/`） | deploy 扩三书 |
| 3 | **命名** learn / reference / internals | URL 与全部链接 |
| 4 | **不引入页头「跟踪」字段**，保持中心化矩阵 + 三问 | §4.1；**别再提这个方案** |
| 5 | **死链检查提到批 0**，其余门禁留批 7 | 批 0 多一条最小检查——搬 200 篇的全程都有网 |
| 6 | **穿插在功能开发之间推进**（不连续做完） | 双轨并存期会拉长 ⇒ 批 0 的总纲必须先说清「过渡期新知识往哪写」 |
| 7 | **`formats/` 独立成 internals 的一个部分** | zbc / zpkg / ir |
| 8 | **全仓链接重指允许脚本批量改 + 人工抽查** | 批 6 |
| 9 | **长设计文档先抢救再删** | 批 1/2/5 各多一道人工阅读 |
| 10 | **`language-overview.md` 拆成 ~10 个主题页** | 批 3 的主要工作量 |
| 11 | **未实施的前瞻设计留在 internals 各部分内**，页头标「设计已定 / 未实施」 | 不另开 `future/` part |
| 12 | **`embedding/` 属 reference** | reference 多一个 part |

### 裁决 6 的直接后果：过渡期纪律

穿插推进意味着 `docs/design/` 与新书会**并存数周甚至更久**。批 0 的总纲必须写死一条，否则又会退回「两份并行漂移」的老病：

> **过渡期铁律**：`docs/design/` 与 `docs/workflow/` 自批 0 起**冻结为只读**。任何人要改其中一篇，
> 必须先把它按清单搬进目标书再改——**不允许原地修改**。新知识一律直接写进三本书。

（这正是现行 D2「不再往 design 写」失败的地方：它只说「不要写」，没说「要改怎么办」，于是
`design/runtime/zbc.md` 被就地改到了 2026-09-16。）

### 裁决 9 的执行方式

三篇待抢救的长文，各自在所属批次内处理，**读完再删、删在同一个 PR**：

| 文档 | 行数 | 抢救什么 | 批 |
|---|---|---|---|
| `design/compiler/compiler-architecture.md` | 1431 | 符号解析优先级链 / workspace 兄弟成员解析 / intra-package 同名降级 fixup（带 2026-06~07 change 名，可能仍成立）→ 并进 `internals/compiler/source-compile.md` | 1 |
| `design/runtime/vm-architecture.md` | 1212 | 对齐点停在 05-20，而 book 的 runtime 21 页覆盖到 09-16 ⇒ **先判定它还剩多少独有内容**（`VmContext`/`VmCore` 一节大概率仍成立），再决定是当主干页还是拆碎并入 | 2 |
| `design/toolchain/build-orchestrator.md` | 190 | 八相位管线 / `ICompiler` in-process 编译 / hook 注入（book 对应页只有 109 行，疑似「标了 ✅ 但没真迁完」）→ 并进 `internals/toolchain/z42b.md` | 5 |

## 六、必须回答的风险

### 6.1 链接会大面积断

约 200 篇文档的相对链接 + `src/**/README.md` 与 `docs/agent/rules/` 里的交叉引用全部要重指。

**对策**（User 裁决 5 后已解决）：**死链检查提前到批 0**（`xtask test docs --links`，只查相对链接可解析），于是批 1–6 每一批都有机械保证；每批 PR 内用脚本重写该批涉及的路径（裁决 8），批 6 再做一次全仓 grep 清零（`docs/design/` 与 `docs/workflow/` 字样必须为 0）。其余门禁项仍留批 7。

### 6.2 已发布 URL 会变

`/z42/`（现 book）将不复存在，内容分流到 `/z42/reference/` 与 `/z42/internals/`。外部书签与既有链接会断。

**pre-1.0 阶段建议直接接受**，不做重定向层（与 philosophy「不为旧版本提供兼容」一致）。若 User 认为需要，可在站点根放一页分流索引。

### 6.3 与 roadmap `infra-extract-user-docs` 的张力

roadmap 有一条 06-15 战略「教程外迁 z42-docs 仓」。本次拆分后，**learn + reference 天然成为可整体外迁的单元**（它们已是面向用户、语种一致、发布独立的两本），internals 留在本仓。这实际上**让外迁更容易**，不冲突——但需 User 确认这个理解。

## 七、Out of Scope

- **`docs/spec/changes/` 的 118 个未归档 change**（其中 19 个已标 🟢）：真实的卫生问题，但属 `docs/spec/` 而非三本书，建议单开 change 清理。
- ~~`.claude/rules/` 的收口~~ → **已由 change `consolidate-agent-rules` 完成**（16 篇归一到 `docs/agent/rules/`）。
- **`xtask test docs`**：User 已裁决排在重构之后（批 7 单列，不并入前六批）。
- **internals 的内容重写**：本次只做**搬迁 + 合并 + 删重复**，不借机重写机制页的内容质量。
