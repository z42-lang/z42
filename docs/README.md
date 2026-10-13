# docs/

z42 项目文档总入口：这里有哪些文档、各自给谁看、想做什么该读哪份。

## 结构

```
docs/
├── learn/          学习手册      用 z42 写程序的人（学）
├── reference/      语言与库参考  用 z42 写程序的人（查）
├── internals/      实现内幕      改 z42 本身的人
├── agent/          AI 协作规范   给大模型的工作流程与约束
├── roadmap.md      项目计划与 Deferred 索引
└── <主题>.md       临时迭代底稿（迭代完删除）

src/**/README.md    目录说明      这个目录有什么、怎么用、怎么测、深入读哪页
README.md           仓库门面      定位 + 上手 + 分流
```

## 三本书

| 书 | 给谁 | 回答什么 | 在线 |
|---|---|---|---|
| [`learn/`](learn/) | 用 z42 写程序的人（**学**）：按顺序读一遍就会用 | 怎么上手做成一件事 | <https://z42-lang.github.io/z42/learn/> |
| [`reference/`](reference/) | 用 z42 写程序的人（**查**）：不读实现也能用对 | 规则是什么、有哪些、怎么写 | <https://z42-lang.github.io/z42/reference/> |
| [`internals/`](internals/) | 改 z42 本身的人：要动这块代码才需要读 | 怎么实现的、为什么这样、在哪改、怎么构建测试 | <https://z42-lang.github.io/z42/internals/> |

三本书各是一个 mdBook，同站发布；站点根（[`book/`](book/)）是三书分流索引。
本地渲染：`cargo install mdbook` 后进对应目录跑 `mdbook serve --open`。

语种：`learn` / `reference` 中文先行，内容稳定后出英文版（另一套 SUMMARY，共用 `examples/`）；
`internals` 只有中文。关键术语一律保留英文原词。

## 配套（不是书）

| 位置 | 是什么 |
|---|---|
| [`agent/`](agent/README.md) | AI 协作规范：大模型（及与之协作的人）干活的流程、判据与约束。系统知识不在这里 |
| [`roadmap.md`](roadmap.md) | 项目计划与 Deferred 索引 |
| 根目录的临时迭代底稿（当前：[`runtime-audit.md`](runtime-audit.md)） | 一轮多 PR 迭代的审计结论、证据、计划与进度；迭代完删除 |
| [`internals/src/features.md`](internals/src/features.md) | 语言特性决策台账（决策 + 状态） |
| 各 `src/**/README.md` | 目录说明：这个目录有什么、怎么用、怎么测，深入去 internals 哪一页 |
| 根 [`README.md`](../README.md) | 仓库门面与分流 |
