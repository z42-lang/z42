# docs/agent/ — AI 协作规范

给大模型（及与之协作的人）的**工作流程、判据与约束**，与具体 AI 工具无关：Claude Code、Codex、Cursor、Gemini 等
都从这里读同一份规范。这里只有规范；系统知识在三本书，目录说明在各级 README——下面「开工」一节告诉你怎么找到它们。

> z42 是一门融合 C#、Rust、Python 优点的系统编程语言：编译器 z42c 用 z42 自举（`src/compiler`，编译为 zpkg），
> 虚拟机用 Rust（Interpreter / JIT / AOT）。当前计划与焦点只看 [docs/roadmap.md](../roadmap.md)，不在别处复制。

**每次会话先读**：本文件 · [workflow.md](rules/workflow.md)（流程主线）· [philosophy.md](rules/philosophy.md)（判断准则）·
[doc-system.md](rules/doc-system.md)（文档规则）。其余按下方索引「什么时候读」进。

**每条规则只在唯一归属文件里定义**，其余位置只留「触发条件 + 一行结论 + 链接」。

---

## 开工：先找 README 和书

**读代码前，先按这个顺序找资料**——README 是入口，书是细节，别从源码盲搜：

1. **定位目录**：查下表，或根 [README](../../README.md#repository-layout) 的 Repository Layout。
2. **读目录 README（必须）**：「功能索引」找入口，「如何测试验证」拿验证命令，「关联文档」指向书里的深入页。
   读某目录的代码前必须先读它的 README（如果存在），再决定读哪些文件。
3. **进书**：要理解机制、决策、在哪改 → internals 对应页（页头「代码」行回指源码）；
   要确认对外行为（语法、API、CLI、诊断）→ reference 对应页。
4. **构建 / 测试 / 调试命令** → [internals · 开发基础设施](../internals/src/devinfra/README.md)
   （[开发环境](../internals/src/devinfra/dev-setup.md) · [怎么跑测试](../internals/src/devinfra/testing.md) ·
   [GREEN gate](../internals/src/devinfra/test-gate.md) · [调试手法](../internals/src/devinfra/debugging.md)）。
5. **找不到**：按子系统翻 [internals 目录](../internals/src/SUMMARY.md)；某目录 README 缺「关联文档」或链错，
   补上它属于本次的文档同步（列入 Scope）。

| 要动什么 | 目录 README | internals | reference | 规则 |
|---|---|---|---|---|
| 编译器 z42c | [src/compiler](../../src/compiler/README.md) → 子包 README | [编译器](../internals/src/compiler/README.md) | [语言](../reference/src/language/README.md) | [compiler-z42c](rules/compiler-z42c.md) · [bootstrap-seed](rules/bootstrap-seed.md) |
| 产物格式 zbc / zpkg / IR | [src/compiler/z42.package](../../src/compiler/z42.package/README.md) | [产物格式](../internals/src/formats/README.md) | — | [version-bumping](rules/version-bumping.md) |
| 虚拟机 z42vm | [src/runtime](../../src/runtime/README.md) | [运行时](../internals/src/runtime/README.md) | [嵌入](../reference/src/embedding/README.md) | [runtime-rust](rules/runtime-rust.md) |
| 标准库 | [src/libraries](../../src/libraries/README.md) → 包 README | [标准库](../internals/src/stdlib/README.md) | [标准库](../reference/src/stdlib/README.md) | [bootstrap-seed 轴③](rules/bootstrap-seed.md) |
| 工具链 z42 / z42b | [src/toolchain](../../src/toolchain/README.md) | [工具链](../internals/src/toolchain/README.md) | [工具链](../reference/src/toolchain/README.md) | — |
| 测试用例 | [src/tests](../../src/tests/README.md) | [测试体系](../internals/src/testing/README.md) | [测试](../reference/src/testing.md) | [workflow 测试要求](rules/workflow.md#测试要求必须遵守) |
| 构建脚本 / CI / xtask | [scripts](../../scripts/README.md) | [开发基础设施](../internals/src/devinfra/README.md) | — | [bootstrap-seed](rules/bootstrap-seed.md) |
| 文档 | [docs](../README.md) | — | — | [doc-system](rules/doc-system.md) · [book-writing](rules/book-writing.md) · [readme-writing](rules/readme-writing.md) |

跨语言的坑（加载顺序、id 作用域）不分子系统，写任何代码前扫一眼 [common-pitfalls](rules/common-pitfalls.md)。

---

## 规范索引

### 推进一次变更时

| 文件 | 管什么 |
|------|--------|
| [workflow.md](rules/workflow.md) | 流程主线（以 PR 为单位）：隔离铁律、阶段 0–6、变更分类、Spec-First、Scope、批量授权、GREEN 门禁、并行与合并顺序、越界防护、测试要求 |
| [commit-log.md](rules/commit-log.md) | 提交信息格式 `type(scope): 描述`、scope 子系统表、页脚 |
| [philosophy.md](rules/philosophy.md) | 做选择时的判准：最终方案优先 / 根因修复 / 不做兼容 / 设计完整性 / 规范冲突检测 / 事实校正 / 延后管理 |

### 写文档时

| 文件 | 管什么 |
|------|--------|
| [doc-system.md](rules/doc-system.md) | 内容归属（README / 书 / agent）、链接方向、三问、三道门、防漂移 |
| [book-writing.md](rules/book-writing.md) | 三本书的页型与写法：页头、机制页骨架、图表、检索；`learn` 的章节写法与 `examples/` 会话脚本 |
| [readme-writing.md](rules/readme-writing.md) | 根 README 与目录 README 的写法（**目录 README 模板的唯一 SoT**） |

### 写代码时

| 文件 | 管什么 |
|------|--------|
| [code-organization.md](rules/code-organization.md) | 哪层目录要 README + 文件/函数/类型的行数限制（`xtask check lines` 棘轮） |
| [common-pitfalls.md](rules/common-pitfalls.md) | 跨语言共同陷阱：加载顺序非确定性、id 作用域 |
| [compiler-z42c.md](rules/compiler-z42c.md) | z42c 代码约定：只用当前语言子集、Parser 错误处理 |
| [runtime-rust.md](rules/runtime-rust.md) | Rust VM 代码约定：错误处理、测试组织、原生代码与 GC 的四条规则、执行模式、依赖引入 |

### 碰自举链 / 改格式时

| 文件 | 管什么 |
|------|--------|
| [bootstrap-seed.md](rules/bootstrap-seed.md) | 种子鸡蛋问题；**新语法 / 格式 / API support 先行、晚一个 nightly 再 use** |
| [version-bumping.md](rules/version-bumping.md) | zbc / zpkg 格式 bump 纪律 + 编译器语义指纹 |

## 各 AI 工具的入口

规范只在本目录维护；各工具的入口文件只做引用，外加该工具专属的用法（如跨会话记忆放哪）。

| 工具 | 入口 | 说明 |
|------|------|------|
| Claude Code | [`.claude/CLAUDE.md`](../../.claude/CLAUDE.md) | 自动载入「每次会话先读」的四份；`.claude/skills/` 是任务清单的斜杠命令薄包装（清单本体在书里）；`.claude/settings.json` 是团队共享权限 |
| 其他工具 | 本文件 | 在该工具的指令文件里指向 `docs/agent/README.md`，并让它先读上面四份 |
