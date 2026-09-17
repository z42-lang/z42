# docs/

z42 项目文档总入口。

## 三本书

判据与协同改动规范见 [`agent/rules/doc-system.md`](agent/rules/doc-system.md)。

| 书 | 受众 | 判据 | 在线 |
|---|---|---|---|
| [`learn/`](learn/) | 用 z42 写程序的人（**学**） | 按学习顺序读一遍就会用 | <https://z42-lang.github.io/z42/learn/> |
| [`reference/`](reference/) | 用 z42 写程序的人（**查**） | **不读实现也能用对** | <https://z42-lang.github.io/z42/reference/> |
| [`internals/`](internals/) | 改 z42 本身的人 / AI | **要动这块代码才需要读** | <https://z42-lang.github.io/z42/internals/> |

三本书各是一个 mdBook，同站发布；站点根（[`book/`](book/)）是三书分流索引。
本地渲染：`cargo install mdbook` 后进对应目录跑 `mdbook serve --open`。

**链接方向是单向的**：`learn → reference`，`internals → learn / reference`。
**面向用户的两本书里不出现任何指向 `internals/` 的链接**（绝对站点 URL 也算）。

## 配套（不是书）

| 位置 | 职责 | 不写什么 |
|---|---|---|
| [`agent/rules/`](agent/rules/) | 怎么干活的**行为约束** | **任何系统知识**——那是三本书的事 |
| [`spec/`](spec/) | 变更工作区（`changes/` 进行中 + `archive/` 已归档） | 长期知识（归档时上浮到三书） |
| [`roadmap.md`](roadmap.md) | 项目计划与 Deferred 索引 | 知识 |
| [`features.md`](features.md) | 语言特性 catalog（决策 + phase 归属） | |
| 各 `src/**/README.md` | 这个目录有什么、怎么改 | 设计原理（链 internals） |
| 根 [`README.md`](../README.md) | 仓库门面与分流 | 实质内容 |

## 语种

- `learn` / `reference`：中文先行，内容稳定后出英文版（另一套 SUMMARY，共用 `examples/`）
- `internals`：中文（内部文档）
- 关键术语一律保留英文原词

## ⏳ 搬迁残留

[`design/`](design/) 已基本清空，仅余两项待裁决 / 待迁：

- `design/philosophy.md` —— 语言设计哲学，待迁入 `internals/`
- `design/stdlib/README-template.md` —— 与 `agent/rules/readme-writing.md` 的六段制冲突，待裁决
