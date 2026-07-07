---
paths:
  - "**/*"
---

# z42 人机协作工作流（OpenSpec 风格）

> Claude 无需任何外部工具即可执行本流程。所有状态通过文件读写维护。

---

## 协作模型

```
Think First → Spec It → Build It → Archive It
  探索思考  →  写规范  →  写代码  →  归档留存
```

**角色分工：**

| 角色 | 职责 |
|------|------|
| **User** | 定方向、审批规范、裁决分歧 |
| **Spec**（`docs/book/` + `docs/spec/`）| 人机合同，实现的唯一依据 |
| **Claude** | 自驱执行各阶段；不超 Scope；不猜测歧义 |

**User 介入点只有两个：** ① 规范审批（Proposal + Spec）；② 规范分歧裁决。其余全部 Claude 自驱。

---

## 目录结构

```
docs/spec/
├── changes/                    ← 进行中的变更提案
│   └── <change-name>/          ← kebab-case，动词开头
│       ├── proposal.md         ← Why：动机与范围
│       ├── design.md           ← How：技术方案与决策
│       ├── specs/
│       │   └── <capability>/
│       │       └── spec.md     ← What：可验证的场景（本变更的 delta）
│       └── tasks.md            ← 实施清单（checkbox）
└── archive/                    ← 已归档变更（与 changes/ 并列）
    └── YYYY-MM-DD-<name>/
```

长期规范（新语法、IR 指令、VM 行为）最终上浮到 `docs/book/`（知识库唯一 SoT），不存在 `docs/spec/` 中。

### 与 OpenSpec 原版的偏离（z42 本地约定）

本工作流脱胎于 [OpenSpec](https://openspec.dev/) 社区方法论，但有四处显式偏离：

| 维度 | OpenSpec 原版 | z42 本地 | 偏离理由 |
|------|-------------|--------|--------|
| **目录名** | `openspec/` | `spec/` | 去掉方法论品牌暗示，名字更中性 |
| **目录位置** | 仓库根 `openspec/` | `docs/spec/`（2026-05-10 起）| spec 与 design doc 同属"项目文档"范畴；放在 `docs/` 下减少顶层目录数，单一文档目录便于检索 |
| **archive 位置** | `changes/archive/` 子目录 | `archive/` 与 `changes/` 并列 | archive 不是一个 change；并列使 "进行中 vs 历史" 语义清晰，扫描活跃变更不需排除子目录 |
| **顶层 specs 库** | `openspec/specs/<capability>/spec.md` 作为系统当前行为的 SoT | 无顶层 `docs/spec/specs/`，长期规范为 `docs/book/` 对应页 | z42 的语言/IR/VM 规范用人类可读的叙事文档组织（给语言使用者读），而非结构化 capability spec；变更归档时知识上浮到 `docs/book/` |

这些偏离一经明确，不得在未经讨论的情况下回改 OpenSpec 原版结构。

---

## 变更分类

| 类型 | 触发条件 | 流程 |
|------|---------|------|
| `lang` | 新语法、关键字、类型规则 | 完整流程（阶段 1–9）|
| `ir` | 新 IR 指令、zbc 格式变更 | 完整流程 |
| `vm` | VM 执行语义变更 | 完整流程 |
| `fix` | Bug 修复，不改语义 | 最小化模式（仅 tasks.md）|
| `refactor` | 纯重构，不改行为 | 最小化模式 |
| `test/docs` | 测试或文档 | 直接实施，无需文件 |

**判断规则：类型优先，文件数作为 fix/refactor 内部细分。**
- `lang` / `ir` / `vm` 类型：**无论文件数量**，一律走完整流程（阶段 1–9）
- `fix` / `refactor` 类型：> 3 个文件 → 最小化模式（tasks.md）；1–3 个文件 → 直接实施；单行 bugfix → 直接修改

---

## 🔴 Spec-First Self-Check（lang / ir / vm 变更强制）

**触发条件**：任何变更涉及新语法、新 IR 指令、新 VM 行为、新关键字、新类型系统规则、新约束机制、新接口契约。

**在写第一行实现代码之前，Claude 必须通过以下 self-check**：

```
[ ] docs/spec/changes/<name>/proposal.md 存在且 User 已确认
[ ] docs/spec/changes/<name>/specs/<capability>/spec.md 存在且 User 已确认
[ ] docs/spec/changes/<name>/design.md 存在且 User 已确认
[ ] docs/spec/changes/<name>/tasks.md 存在
[ ] 阶段 6.5 实施前确认 gate 已通过（User 明确说"没问题 / 可以开始"）
```

**任一未达成 → 停，回到阶段 1–6 补齐，不得推进代码。**

**常见反例（皆为违规）**：
- ❌ 只有 `docs/book/` 对应页（长期规范）就开始写代码
  → 长期规范 ≠ `docs/spec/changes/<name>/` 的 proposal/specs/design；两者都必须有
- ❌ "因为迭代中与 User 逐步沟通了方案，所以跳过 proposal/specs"
  → 对话中的确认不替代 spec；User 审批的是文档，不是聊天记录
- ❌ 只建 `tasks.md` 就开工（除非明确是 fix / refactor 类型）
  → lang/ir/vm 类型不能走最小化模式
- ❌ 写完代码后再补 proposal/specs
  → spec 的作用是在实施前定义边界，事后补齐只剩文档价值、失去约束价值

**如果拿不准变更类型**：优先按严格模式走（完整流程）。多写 3 个文档的代价远小于跳过 spec 造成的返工和边界漂移。

---

## 阶段 0：意图识别

**每次新对话首条消息触发：** Claude 自动读取 `.claude/projects/<project>/memory/MEMORY.md`
和当前阶段（roadmap + `docs/spec/changes/` 进行中变更），主动汇报状态和下一步，再处理用户输入。

### 阶段 0 必做：扫描可归档变更（2026-06-19 新增）

**在处理用户指令前，Claude 必须先扫描 `docs/spec/changes/` 下的全部 change，识别并归档"实质已完成"的 change：**

**可归档的判定标准：**
1. **tasks.md 阶段摘要（进度概览）全部 [x]** 且目标已验证（GREEN 通过 / 对账面板 / byte-identical 确认等）
2. **所有 [ ] 均属"被后续任务明确覆盖的早期诊断记录"**——后续某个 task 说"清零 / 修复 / 顺带清掉"了该条，则视为已完成
3. **tasks.md 头部已标 🟢 已完成** 但尚未 move 到 archive

发现可归档 change 时：
- 将被覆盖的 stale `[ ]` 改为 `[x]`（注明由哪个后续任务解决）
- 更新 tasks.md 头部状态为 `🟢 已完成 | 完成：YYYY-MM-DD`
- 执行阶段 9 归档动作（mv + ACTIVE.md 更新）
- 归档提交并入当次会话第一个 commit，不单独占新对话

**检测来源（无需逐行细读，粗扫就够）：**
- tasks.md 头部状态标志（🟢 / 🟡 / 🔴）
- `grep -c '^\- \[ \]' tasks.md` 的未勾数 vs tasks.md 末尾 G/子任务的"完成声明"
- `docs/spec/changes/ACTIVE.md` 子系统持有表是否有与 `spec/changes/` 不符的陈旧条目

Claude 读到以下关键词时自动触发对应动作：

| 用户说 | Claude 做 |
|--------|-----------|
| "我想做 X" / "实现 Y" | 阶段 1（探索）开始 |
| "继续" / "下一步" | 状态恢复协议 |
| "直接做" / "快速开始" | 最小化模式：只写 tasks.md（**仅 fix/refactor 类型适用**） |
| "开始写代码" / "实施" | 阶段 7（须先通过阶段 6.5 确认）|
| "没问题" / "可以开始" | 阶段 6.5 通过 → 进入阶段 7 |
| "批量确认" / "一并授权" / "这一系列都按计划做" / "自动推进" | 阶段 6.5 批量授权模式启动 |
| "停" / "暂停" / "不要继续" | 终止批量授权，进入交互模式 |
| "验证一下" | 阶段 8 |
| "归档" / "完成了" | 阶段 9 |
| "分析一下" / "探索" | 阶段 1，不创建任何文件 |

**词汇警报（lang/ir/vm 关键词触发强制完整流程）**：
检测到 "新语法 / 新关键字 / 新 IR 指令 / 新约束 / 新接口契约 / 新类型规则 /
新 VM 行为" 等描述时，**不论用户用什么语气**（即便说"快速开始"/"直接做"），
都必须走阶段 1–9 完整流程，创建 proposal + specs + design + tasks，不得
降级到最小化模式。

---

## 阶段 1：探索（Explore）

Claude 执行（不创建文件，仅输出）：

1. 读取相关源文件，理解现有结构
2. 梳理核心问题 / 需求
3. 列出潜在风险和边界情况
4. 提出 2–3 个可行方案（如有）
5. 给出推荐及理由
6. **等待 User 选择方案**，再进阶段 2

**z42 专项检查：**
- 确认属于哪个实现阶段（Phase 1/2/3），Phase 3 特性拒绝推进
- 确认影响的 pipeline 组件（Lexer / Parser / TypeChecker / Codegen / VM interp / JIT）

---

## 阶段 2：创建变更容器

```
docs/spec/changes/<change-name>/
├── proposal.md
├── design.md
├── specs/<capability>/spec.md
└── tasks.md
```

命名：动词开头，kebab-case，≤ 5 词。如 `add-for-loop`、`fix-type-check-crash`。

**并行占用登记（必做）**：创建容器同时，按 [`parallel-development.md`](parallel-development.md) 声明本 change 占用的子系统，逐个查 `docs/spec/changes/ACTIVE.md`——任一被占则**停下排队**，全部空闲才登记为持有者并继续。

---

## 阶段 3：编写 proposal.md（Why）

草稿展示给 User → 确认后写入文件。

```markdown
# Proposal: <标题>

## Why
[1–3 句：背景和问题，不做会怎样]

## What Changes
- [变更点列表]

## Scope（允许改动的文件）

**必须列出每个具体文件路径**（不允许只写"模块名"或"目录名"）。每个文件必须能被 tasks.md 中至少一个 task 命中。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/path/to/Foo.z42`    | NEW    | 新增文件 |
| `src/path/to/Bar.z42`    | MODIFY | 修改 X 字段 / Y 方法 |
| `src/path/to/Old.z42`    | DELETE | 删除（pre-1.0 直接删，不留兼容） |
| `docs/book/src/<part>/foo.md` | MODIFY | 同步知识库 |
| `src/path/to/tests/foo/source.z42` | NEW | 新测试 |

**只读引用**（理解上下文必须读，但不修改；不计入并行冲突）：

- `src/path/to/Existing.z42` — 用于理解 X 行为
- `docs/book/src/<part>/related.md` — 参考 Y 规则

**变更类型枚举**：`NEW` / `MODIFY` / `DELETE` / `RENAME`（rename 同时占用旧路径 DELETE + 新路径 NEW）。

**并行 spec 冲突检测规则**（用于判断多个 spec 是否能并行实施）：

| 情况 | 是否冲突 |
|------|--------|
| 同一文件同时出现在两个进行中 spec 的 Scope（任一非只读引用） | ✅ 冲突 → 必须串行 |
| 同一文件仅在一个 spec 的 Scope，另一个 spec 只是只读引用 | ❌ 不冲突 → 可并行 |
| 两个 spec 的 NEW 文件路径在同一具体子目录（如同时在 `src/Foo/` 下创建文件） | ⚠️ 视目录性质判断；通用容器（`src/`）不冲突，专属子模块（`src/Foo/Bar/`）按团队约定 |
| 两个 spec 都修改同一 markdown 文档的不同段 | ✅ 冲突 → 串行（避免 merge 冲突） |

> 拿不准时按冲突处理（串行实施），代价远低于事后 merge 冲突排查。

> **并行执行（src 代码）**：上表是 docs/markdown 的细则；`src/` 代码的并行判定改用**子系统互斥锁**——见 [`parallel-development.md`](parallel-development.md) + `docs/spec/changes/ACTIVE.md` 账本。开 change 前查账本，子系统被占则排队。

## Out of Scope
- [明确排除，防止范围蔓延]

## Open Questions
- [ ] [待确认问题]
```

**Scope 表必须满足的硬性约束：**

- 路径必须是项目内可解析的**具体路径**（不允许 `src/compiler/*` 这种通配，也不允许 `相关测试文件` 这种模糊描述）
- 每条 NEW / MODIFY / DELETE 必须能被 tasks.md 中**至少一项 task** 引用
- 反过来：tasks.md 中触及的所有文件必须**全部**列入 Scope；实施中临时发现需要改动的文件 → **立即停下回到阶段 3** 更新 Scope（违反 = 越界）

---

## 阶段 4：编写 specs/spec.md（What）

用可验证的场景定义行为。草稿展示 → User 确认 → 写入。

```markdown
# Spec: <Capability 名称>

## ADDED Requirements

### Requirement: <需求名>

#### Scenario: <正常场景>
- **WHEN** <触发条件>
- **THEN** <预期结果>

#### Scenario: <边界 / 异常场景>
- **WHEN** <条件>
- **THEN** <结果>

## MODIFIED Requirements
（修改现有行为时）
**Before:** <原行为>
**After:** <新行为>
```

**z42 专项（lang/ir 类型必须包含）：**

```markdown
## IR Mapping
[新语法对应的 IR 指令 / zbc opcode]

## Pipeline Steps
受影响的 pipeline 阶段（按顺序）：
- [ ] Lexer
- [ ] Parser / AST
- [ ] TypeChecker
- [ ] IR Codegen
- [ ] VM interp
```

---

## 阶段 5：编写 design.md（How）

技术方案和决策记录。草稿展示 → User 确认 → 写入。

```markdown
# Design: <标题>

## Architecture
[ASCII 图或组件关系描述]

## Decisions

### Decision 1: <标题>
**问题：** ...
**选项：** A — 优/缺；B — 优/缺
**决定：** 选 X，因为 ...

## Implementation Notes
[关键 API、注意事项、边界条件]

## Testing Strategy
- 单元测试：[覆盖点]
- Golden test：[新增场景]
- VM 验证：xtask test（cargo build z42vm + z42c 自建 + vm/cross-zpkg/stdlib/compiler）
```

---

## 阶段 6：编写 tasks.md（实施清单）

```markdown
# Tasks: <标题>

> 状态：🟡 进行中 | 创建：YYYY-MM-DD

## 进度概览
- [ ] 阶段 1: 基础
- [ ] 阶段 2: 核心实现
- [ ] 阶段 3: 测试与验证

## 阶段 1: 基础
- [ ] 1.1 [具体任务，指定文件和方法]

## 阶段 2: 核心实现（按 pipeline 顺序）
- [ ] 2.1 Lexer/Token（如有）
- [ ] 2.2 Parser/AST 节点
- [ ] 2.3 TypeChecker
- [ ] 2.4 IR Codegen
- [ ] 2.5 VM interp（interp 全绿前不碰 JIT）
- [ ] 2.6 单元测试 + golden test
- [ ] 2.7 examples/ 示例文件

## 阶段 3: 验证
- [ ] 3.1 cargo build (z42vm) —— 无编译错误（z42c + stdlib 由 xtask test 用 z42c 自建）
- [ ] 3.2 xtask test compiler —— z42c 自举全绿
- [ ] 3.3 xtask test e2e —— 全绿
- [ ] 3.4 spec scenarios 逐条覆盖确认
- [ ] 3.5 文档同步（按阶段 9 触发矩阵：目录 README / `docs/book/` / workflow）
- [ ] 3.6 docs/roadmap.md 进度表更新（若有特性完成某 pipeline 阶段）

## 备注
[实施中发现的问题 / 决策变更]
```

**任务粒度：** 每项对应一个明确的代码操作，30 分钟内可完成。每完成一项立即将 `[ ]` 改为 `[x]`。

---

## 阶段 6.5：实施前确认（Gate）

### 单 spec 模式（默认）

**规范文档（proposal / spec / design / tasks）全部就绪后，Claude 必须向 User 展示完整方案摘要并明确询问：**

```
## 实施前确认

以下规范已就绪，请确认是否有问题：

- **Proposal:** [一句话]
- **Spec:** [场景数量] 个验证场景
- **Design:** [关键决策摘要]
- **Tasks:** [N] 项任务

请确认是否有问题？有问题请指出，没问题我开始实施。
```

**规则：**
- User 明确说"没问题"、"可以"、"开始"等肯定回复后，才能进入阶段 7
- User 提出问题 → 修改对应文档 → 重新展示摘要 → 再次询问，**循环直到确认**
- **不得跳过此步骤**，即使 User 之前逐步确认了每个文档
- 最小化模式（fix / refactor）同样适用：展示 tasks.md 摘要 → 确认 → 实施

### 批量授权模式（Batch Approval）

**触发条件**：当一项规划被显式拆成多个 spec（如 C1 / C2 / C3 / C4），且 User 明确说"批量确认"、"一并授权"、"这一系列都按计划做"、"自动推进，不用每次问"等表述时启用。

**激活流程：**

1. Claude 一次性展示**所有受授权 spec 的摘要**（每个 spec 一段，含 proposal + spec scenarios 数 + design 关键决策 + tasks 项数）
2. User 明确说"全部开始 / 没问题 / 批量授权" → 所有受授权 spec 进入待实施队列
3. Claude 在内存中记录批量授权范围（spec 名单 + 已确认时间），后续切换 spec 时无需重新询问

**激活后 Claude 行为：**

- 按 spec 间依赖顺序逐个进入阶段 7 实施 → 阶段 8 验证 → 阶段 9 归档
- 每个 spec 归档后**自动开始下一个**，仅需汇报状态切换：

  ```
  ✅ C1 已归档（commit: <hash>），1/4 完成
  🟡 现在开始 C2: <name>
  ```

- 不再为每个 spec 单独执行阶段 6.5 的"展示摘要 + 询问"
- 不再在每次 commit / push 后等待 User 确认

### 批量授权的中断条件（必须停下询问）

任一事件发生 → Claude 立即停下，向 User 汇报，等待裁决，不得视为"已经全部授权"而自行决定：

1. **Scope 越界**：实施中发现需要修改授权 Scope 之外的文件 → 回阶段 3 更新当前 spec 的 Scope，或开新 spec
2. **测试失败超出当前 spec 范围**：发现 pre-existing failure 或外部回归
3. **规范冲突**：实施中发现两个 spec 的设计相互冲突，或与 `docs/book/`（迁移期含旧 design/）现有规范冲突
4. **决策点未覆盖**：spec 中未明确的设计点（如字段命名、错误信息措辞、性能权衡），不得自行决定
5. **依赖前置变更需调整**：例如 C1 落地后发现 C2 引用的 C1 字段需要重命名
6. **GREEN 失败**：当前 spec 验证未全绿（参见阶段 8）
7. **超出预期工作量**：某 spec 的实际任务量明显超出 tasks.md 估计（如 1.5x 以上）
8. **架构性发现**：实施中发现某个原本认为是局部的变更其实牵涉跨模块重构

### 批量授权下仍然必须的步骤

- 每个 spec 独立通过 GREEN 验证（阶段 8）才能进入下一个
- 每个 spec 单独 commit + push（不积压、不混合）
- 每个 spec 完成后立即归档到 `docs/spec/archive/`
- `.claude/` 与 `docs/spec/` 必须纳入提交
- 任何中断条件触发时立即停下

### 批量授权的边界声明

- **批量授权 ≠ 自动扩张授权**：授权范围**严格限于一开始展示并被 User 明确确认的 spec 名单**；新发现的需求必须重新走"提议 + 单 spec 确认"或"扩展批量名单 + 重新确认"
- 批量授权对"代码实施 + commit + push"生效；对**外部影响动作**（如 force-push、删除分支、改 CI 配置）仍需单独确认
- User 任何时候说"停"、"暂停"、"不要继续了" → 立即终止批量授权，进入交互模式

---

## 阶段 7：实施（Apply）

对每个任务：
1. 宣告："正在处理 N.M: [描述]"
2. 读取相关文件
3. 实施代码变更
4. 将 tasks.md 中对应项 `[ ]` → `[x]`
5. 简短说明完成情况
6. 遇到阻塞 → 记录到 tasks.md 备注区 + 告知 User

**回溯勾销（2026-06-19 新增）**：当一个新任务（G-N / 子任务）的完成说明中包含"清零 / 修复 / 顺带清掉 / 覆盖"某个早期未勾 `[ ]` 条目时，**必须在同一步骤内**将该早期条目改为 `[x]`，并在其描述中补注 "（由 G-N/子任务 X 解决）"。不允许让"已被后续任务解决的 [ ] 条目"在 tasks.md 中继续存在，否则会造成"progress summary 全绿 but tasks 有 [ ]"的误导，导致 change 无法正确识别为可归档。

**z42 pipeline 顺序（不跳步）：** Lexer → Parser → AST → TypeChecker → Codegen → VM interp → 测试

**规范偏差时：** 立刻停，不猜，不绕过。列出冲突 → User 裁决 → 更新 Spec → 继续。已验证部分不回头重写。

**实施过程中的验证要求：**

- 每个 refactor / fix / feature 实施后，立即在本地运行编译 + 测试
- 发现任何测试失败：
  - ✅ 若是当前变更导致 → 立即修复
  - ✅ 若是 pre-existing 失败 → 在本迭代修复，或明确说明原因后 User 确认
  - ⚠️ 若与当前 Scope 无关 → 记入 tasks.md 备注，新建独立 issue
- 不得跳过任何测试失败后继续实施下一个任务
- 整个阶段 7 完成后，进阶段 8 前必须全绿

---

## 阶段 8：验证（Verify）

**全绿（GREEN）标准：**

任何迭代进阶段 9 前，必须通过以下全部验证步骤，且 **所有测试全部通过**。
统一入口：

```bash
xtask test          # 默认串联所有必跑 stage（完整 GREEN gate）
```

**Scope-aware 加速（add-test-split-by-area, 2026-05-21）**：
iteration 期可用 `--scope=runtime|compiler|stdlib|auto` 缩窄 scope 跳过
不相关 stage。但 **commit 前最终 GREEN 必须 `--scope=full`**（或
`--scope=auto` 没有缩窄到比变更范围窄）。Partial scope 验证只算 dev 期
快速 iterate，不替代 commit 门禁。详细 scope 说明见
[`docs/workflow/testing/README.md`](../../docs/workflow/testing/README.md)。

裸 `test` 先跑 **regen 构建波**（stdlib + z42c 自建 + golden `.zbc` 基线 + debug VM；
`--no-build` 可跳过），随后按顺序跑以下 stage（任一失败立刻停）：

```bash
# 1. 编译验证（无编译错误）—— z42vm（Rust VM）。z42c（编译器）+ stdlib 由 xtask 在下面的
#    test stage 内用 z42c 自建
cargo build --manifest-path src/runtime/Cargo.toml --release

# 2. VM goldens（interp；JIT 由 CI test-vm-jit(linux-x64) 专腿覆盖，job key: vm-jit-consistency）
xtask test e2e

# 3. 跨 zpkg 端到端（catch / vcall / 元数据跨包行为）
xtask test e2e --dir cross-zpkg

# 4. stdlib [Test] dogfood（全量 [Test] 用例）
xtask test lib

# 5. z42c 自举（编译器正确性：build 7 子包 + 产物存在 + [Test] units）
xtask test compiler

# 6. VSCode grammar ↔ Lexer 关键字一致性（生成产物防漂移；Lexer 加关键字未重新生成即红）
xtask test vscode-syntax
```

> **不要漏跑 cross-zpkg / lib / compiler**。historic regression：cross-zpkg
> subclass catch bug 之所以一直没被发现，就是 3 / 4 不在默认 GREEN 路径里 —— 每次
> spec 验证都漏跑。该 lesson 现在以 `xtask test` 形式固化；编译器正确性
> 由 stage 5（z42c 自举）保证。

打包发行验证：发行版变更（xtask build package / 跨平台 / 嵌入接口）追加跑
`xtask test dist`（要求先跑 `xtask build package release`
产 host-RID 包）。

**测试失败处理规则：**

| 情况 | 处理方式 |
|------|---------|
| **当前变更导致的新失败** | ❌ 必须在本迭代修复，不得 commit |
| **当前变更触发的隐藏 bug** | ❌ 必须修复，或明确说明理由后 User 确认 |
| **Pre-existing 失败（变更前已存在）** | ⚠️ 必须在 **同一迭代** 修复，或单独 issue 跟踪 |
| 发现的问题与当前 Scope 无关 | ✅ 记入 tasks.md，新建独立变更，不阻塞本迭代 |

**验证报告（Claude 必须输出）：**

```markdown
## 验证报告

### xtask test 状态：✅ 全绿（N stages）/ ❌ 失败 at <stage>

逐 stage（出现失败时展开）：
- ✅ cargo build (release) —— z42vm
- ✅ xtask test e2e: M/M（GREEN gate `test all` 跑 interp；JIT 由 CI test-vm-jit(linux-x64) 专腿 / 本地 `test e2e --mode jit` 覆盖）
- ✅ xtask test e2e --dir cross-zpkg: K/K
- ✅ xtask test lib: 22/22 lib
- ✅ xtask test compiler: 7/7 zpkg + units（z42c 自举）
- ✅ xtask test vscode-syntax（grammar ↔ Lexer 一致）
- （可选）✅ xtask test dist: P/P

### Spec 覆盖（若有 spec）
| Scenario | 实现位置 | 验证方式 | 状态 |
|----------|---------|---------|------|
| [场景名] | File.z42:行号 | [单元/golden/端到端] | ✅ |

### Tasks 完成度：N/N ✅

### 结论：✅ 全绿，可以归档 / ❌ 未全绿，待修复：[列出]
```

**全绿才能进阶段 9。** 任何未全绿的状态不得 commit / push。

---

## 阶段 9：归档（Archive）

1. 将 tasks.md 状态改为 `🟢 已完成`，更新日期
2. 移动目录：`docs/spec/changes/<name>/` → `docs/spec/archive/YYYY-MM-DD-<name>/`
   - **释放子系统锁**：从 `docs/spec/changes/ACTIVE.md` 摘除本 change 持有的全部子系统行（见 [`parallel-development.md`](parallel-development.md)）
3. **文档同步（统一维护触发矩阵）**：下表是"改了什么 → 必须同步哪些文档"的**唯一 SoT**——
   其他规范只链接此表，不另立分表；"同步到哪一段"的段级展开见各写作规范。逐行核对，
   本次改动命中的行全部落实。知识类内容一律落 `docs/book/`（旧 `docs/design/` 不再更新；
   迁移期若对应 book 页尚不存在，直接把该主题新写进 book，顺带完成迁移）。

   | 改了什么 | 目录 README（六段） | book | 其他 |
   |---------|--------------------|------|------|
   | 新增 / 删除文件，对外入口或依赖变化 | 功能索引 + 核心文件 | — | — |
   | 测试方式 / 测试命令变化 | 如何测试验证 | — | `docs/workflow/testing/` 对应页 |
   | 对外行为变更（新语法 / API / CLI / 二进制格式） | 功能索引 | 对应机制页 / 参考页（知识上浮） | 根 README 对应段；`docs/roadmap.md`（pipeline 进度 / Deferred 索引，如涉及） |
   | 内部机制 / 架构策略变更（数据结构、算法、加载策略、决策权衡） | — | 对应机制页「机制 / 实现」节 | — |
   | 构建 / 打包 / 调试操作变化 | 基础用法（如涉及） | — | `docs/workflow/` 对应页 |
   | change 归档引入新能力 | 关联文档段登记 change 名 | 所改页页头「对齐」日期刷新 | — |
   | 新协作规则 / 流程变化 | — | — | `.claude/rules/` 或 `docs/agent/rules/` 对应文件 |
   | 语言设计决策变更（设计目标 / phase 归属 / 设计理由） | — | 语言部分对应页 | `docs/features.md` |

   **归档前 doc-check 清单（全部勾上才能进 commit 步骤）：**

   - [ ] 触发矩阵逐行核对，命中行的文档均已更新
   - [ ] 所改目录的 README 六段齐全、与本次改动对齐（六段制见 [code-organization.md](code-organization.md)）
   - [ ] 所改 / 新写 book 页：页头「对齐」日期已刷新、代码路径可解析；新页已挂入 `docs/book/src/SUMMARY.md`
   - [ ] 本次触及文档中的相对链接均可解析（无死链）

   > **规则：任何改变了外部可见行为、机制、规则或约定的迭代，归档前必须有对应文档落地。**
   > 无文档 = 未完成，不得进入 commit 步骤。

4. **自动提交（无需 User 确认）**，包含以下所有相关文件：
   ```bash
   git add src/ docs/ examples/ \
           .claude/ \
           .gitignore *.md
   git commit -m "type(scope): 描述"
   git push origin main
   ```
   - `.claude/`（workflow、memory、规则变更）和 `docs/spec/`（proposal、design、spec、tasks、archive；已被 `docs/` 覆盖）**必须纳入提交**，不得遗漏。
   - 每个逻辑单元单独提交，不积压。

---

## 会话恢复协议

User 说"继续"时，Claude 自动执行：

1. 扫描 `docs/spec/changes/`（排除 `archive/`）
2. 读取进行中变更的 `tasks.md`，找到第一个未勾选任务
3. 读取 `CLAUDE.md` → 当前阶段约束
4. 读取 `memory/` → 跨会话决策
5. 汇报：

```
当前变更：<name>
已完成：X/N 项
下一步：任务 N.M — [描述]
继续？
```

6. User 确认后继续

---

## 越界防护

**必须停下询问（不得擅自决定）：**
- 需要改动 Scope 外的文件（**即使在批量授权模式下也必须停**）
- 发现 Scope 外的 Bug → 记录到 tasks.md 备注，新建独立变更，不顺手修
- 规范未覆盖的接口 / 架构选择
- Done Condition 不明确
- 批量授权模式下任一中断条件触发（参见阶段 6.5"批量授权的中断条件"）

**Claude 自主决定（无需询问）：**
算法细节、数据结构选择、代码风格、变量命名。

**Scope 表的强约束：**
- proposal.md Scope 表是 Claude 在阶段 7 实施时**唯一允许触及的文件清单**
- 实施时发现需改动 Scope 外文件 → 立即停下，回到阶段 3 更新 Scope（哪怕只是一行小改），更新后必须 User 重新确认（批量授权下也是）
- "顺手改一下"、"反正在附近"、"应该没人在意" 都是违规，无例外

---

## 最小化模式（fix / refactor）

```
docs/spec/changes/<name>/tasks.md   ← 必须有
```

tasks.md 顶部：

```markdown
# Tasks: <名称>
**变更说明：** [一句话]
**原因：** [一句话]
**文档影响：** [列出需要更新的文档，无则写"无"]

- [ ] 1.1 [任务]
- [ ] 1.x 文档同步（若有行为/机制变更，按阶段 9 触发矩阵）
```

完成后直接进阶段 8（验证）→ 阶段 9（归档）。

> **即使是 fix，只要改变了行为、机制或约定，就必须同步文档。** 只有纯内部实现调整（如重命名变量、提取方法、不改接口）才可跳过文档步骤。

---

## 实现哲学 / 设计完整性 / 延后管理

以下三类规则不属于流程主线，独立沉淀在 [philosophy.md](philosophy.md)：

- **实现方案原则**（优先最终方案 / 不为破坏性顾虑而牺牲 / 修复必须从根因出发 / 不为旧版本提供兼容）
- **设计完整性原则**（设计无法承载需求时停下讨论，禁止打补丁）
- **延后特性管理**（design doc Deferred 段 + roadmap 索引）

具体 `.zbc` / `.zpkg` 格式 version bump 同步 checklist（哪些文件必须改、commit 前自检命令）见 [version-bumping.md](version-bumping.md)。

---

## 测试要求（必须遵守）

**每次新增需求或迭代（非纯文档/纯注释变更），必须包含对应的测试用例。无测试 = 未完成。**

| 变更类型 | 测试要求 |
|---------|---------|
| 新功能 / 新 pipeline 阶段 | 至少 1 个正常用例 + 1 个边界/异常用例的单元测试或 golden test |
| Bug fix | 至少 1 个回归测试，覆盖修复的 bug 场景 |
| 新 IR 指令 / VM 行为 | golden test (run/) 验证端到端执行结果 |
| 新 CLI 命令 / 工程文件字段 | 单元测试验证解析正确性 + 错误输入报错 |
| refactor | 确保已有测试仍覆盖（不新增测试即可，但不得删除测试） |

**测试位置：**
- z42c 编译器：`src/compiler/<member>/tests/<name>/`（如 `z42c.semantics/tests/`、`z42c.syntax/tests/`）
- Rust VM：`src/tests/<category>/<name>/`（VM e2e）或 `src/runtime/src/*_tests.rs`（Rust 单测）；stdlib-bound 用例放 `src/libraries/<lib>/tests/<name>/`
- 跨语言端到端：golden test（source.z42 + expected_output.txt）

---

## 禁止行为

**必须遵守，违反即为严重工作流缺陷：**

- **Spec 未经 User 确认前写实现代码**
  - 规范驱动：所有非平凡变更必须先有 Spec（proposal + specs/<capability>/spec.md + design），User 批准后才开始代码
  - 参见本文件顶部 **🔴 Spec-First Self-Check** 小节 — lang/ir/vm 变更开工前逐项核对
  - **反例**（曾在 2026-04-24 静态抽象接口成员变更中发生）：只建 `tasks.md` +
    长期规范文档就开始实施，把聊天中的逐步确认当作 spec 审批；
    归档时被发现违规。纠正：长期规范（`docs/book/`）与 `docs/spec/changes/<name>/`
    变更规范是**两份独立文档**，lang/ir/vm 变更两者都必须存在

- **验证未全绿时 commit / push**
  - 🔴 **任何测试失败都不得进入 commit**
  - 包括 pre-existing 失败：发现后必须修复，或新建单独 issue + 说明
  - 验证命令必须完整运行：`cargo build && xtask test`（全 stage gate）
  - 全绿的定义：所有编译无错，所有测试 100% 通过

- **顺手修复 Scope 外问题**
  - Scope 内改动优先完成 + 验证
  - 发现的外部问题记入 tasks.md 备注，新建独立变更，不阻塞本迭代

- **用"我理解你的意思是…"绕过歧义**
  - 存在歧义时停下来询问，而不是主观推断

- **interp 未全绿时填 JIT / AOT 实现**
  - VM 实现严格按顺序：interp ✅ → JIT → AOT

- **Phase 3 特性混入 Phase 1/2**
  - 检查 docs/features.md，确认当前 phase 限制

- **单次提交积压多个逻辑单元**
  - 每个 docs/spec/changes/ 变更对应一个 commit
  - 不得在一个 commit 中混合多个独立的功能或修复

- **未经 User 确认擅自采用临时方案**
  - 存在"临时"与"最终"方案时，默认实施最终方案
  - 若条件不允许（依赖未就绪、工作量超出 Scope），必须先向 User 说明 + 得到确认

- **批量授权下扩张授权范围**
  - 批量授权严格限于 User 明确确认的 spec 名单
  - 实施中发现需要新增 spec、改动 Scope 外文件、调整既定决策 → 必须停下询问
  - "反正用户已经说全部授权了" 是错误理解；授权对象是**当时展示的内容**，不是"所有相关工作"

- **Scope 表使用模糊描述**
  - ❌ "src/compiler/* 相关测试"、"涉及的辅助函数"、"对应的文档"
  - ✅ 具体文件路径，每条对应至少一个 task；与 tasks.md 双向对齐
