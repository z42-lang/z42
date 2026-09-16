# Proposal: 单文件运行 `z42 run hello.z42`

## Why

学习手册第 2 章「Hello, World」现在的第一步是 `z42 new hello` —— 读者还没写过一行 z42，就先被要求理解
工程目录、`z42.toml` 的两个段、`namespace`、`src/**/*.z42` 这套 glob。**上手路径的第一级台阶太高。**

实测（当前 SDK）：一个能跑的 z42 程序最小只需要 5 行，**连 `namespace` 都不需要**——

```z42
using Std.IO;

void Main() {
    Console.WriteLine("Hello, World!");
}
```

缺的只是「把这个文件直接跑起来」的命令。补上 `z42 run hello.z42` 后，第 2 章就能是「装好 → 写 5 行 →
跑起来」，工程与清单的概念推迟到第 3 章、在**真正需要**它们时（多源文件、依赖、发布）再引入。

这也是 `add-beginner-cli-onramp`（已归档）规划的阶段 4，当时只做了前三阶段；`docs/design/runtime/launcher.md`
的 Deferred 条目 `launcher-future-single-file-exe-zpkg` 已标注「已排期，由后续 change `add-single-file-run` 实现」。

## What Changes

- **`z42 run <file>.z42 [-- args]`**：在缓存目录里合成最小清单，复用现有工程 `run` 路径（增量缓存、入口
  自动检测、runtimeconfig 一并复用）。**不**把源文件复制进缓存目录。
- **`z42 <file>.z42` 简写**：路由已有 `.zpkg` / `.zbc` 简写，扩到 `.z42`。
- **`SourceDiscovery` 接受清单外的字面文件路径**（当前：绝对路径 include 静默匹配不到，`z42c build:
  no sources matched [sources].include`）。
- **诊断路径相对当前目录**：单文件模式下源文件位于清单目录之外，若原样打印会变成绝对路径，与工程模式的
  `./src/Main.z42` 不一致，且让手册 transcript 跨机器不可复现（门禁在示例目录里重放）。
- **学习手册第 2 章改为单文件**，新增第 3 章「工程与构建」承接 `z42 new` 与 `z42.toml`；`examples/` 随之
  重排。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/launcher/core/launcher.z42` | MODIFY | `_cmdRun` 单文件分支、合成清单、缓存目录解析、`run` 帮助文案 |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | `.z42` 简写路由（现只认 `.zpkg` / `.zbc`） |
| `src/libraries/z42.project/src/SourceDiscovery.z42` | MODIFY | `_expand` 增加「rooted 字面文件路径直通」分支 |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | 诊断/警告里的源路径按当前目录相对化（D2） |
| `src/libraries/z42.project/tests/source_discovery_glob.z42` | MODIFY | 新增 rooted 字面路径用例（命中 / 不存在 / 与 glob 混用） |
| `scripts/test/xtask_test_dist_cli.z42` | MODIFY | 单文件冒烟：运行、`--` 传参、简写、诊断路径、无依赖声明报错 |
| `docs/learn/src/getting-started/hello-world.md` | MODIFY | 第 2 章重写为单文件 |
| `docs/learn/src/getting-started/projects.md` | NEW | 第 3 章「工程与构建」 |
| `docs/learn/src/SUMMARY.md` | MODIFY | 挂入第 3 章 |
| `docs/learn/OUTLINE.md` | MODIFY | 第 2/3 章要点与状态调整 |
| `examples/getting-started/hello-world/hello/hello.z42` | NEW | 第 2 章主示例（单文件） |
| `examples/getting-started/hello-world/hello/run.console` | NEW | 第 2 章会话：`cat` + `z42 run hello.z42` |
| `examples/getting-started/hello-world/greet/` | MODIFY | 改为单文件形态（删 `z42.toml` / `src/`） |
| `examples/getting-started/hello-world/typo/` | MODIFY | 改为单文件形态 |
| `examples/getting-started/hello-world/new/` | DELETE | `z42 new` 会话移入第 3 章 |
| `examples/getting-started/projects/` | NEW | 第 3 章示例（`z42 new` 会话 + 多源文件工程 + build/clean 会话） |
| `docs/book/src/toolchain/cli.md` | MODIFY | `z42 run` 补单文件用法 |
| `docs/design/runtime/launcher.md` | MODIFY | 删除 Deferred 条目 `launcher-future-single-file-exe-zpkg`，改写为已实现 |

**只读引用**（理解上下文必须读，不修改）：

- `src/libraries/z42.project/src/ManifestLocator.z42` / `BuildLayout.z42` — 复用其定位与产物布局语义
- `src/compiler/z42c.driver/src/BuildCommand.z42` — `--quiet` 与参数解析现状
- `docs/spec/archive/2026-09-16-add-beginner-cli-onramp/design.md` — D6 原方案
- `docs/agent/rules/learn-writing.md` — 手册与 examples 写法、门禁规则码
- `scripts/test/xtask_examples_*.z42` — transcript 门禁的沙箱与匹配规则

## Out of Scope

- **单文件支持依赖声明**：单文件 = 只能用 stdlib，与默认工程模板一致；需要依赖时报错提示 `z42 new`。
- **playground**：只保留 `z42,example=<path>` 信息串约定，按钮不做。
- **缓存回收**（`z42 clean --cache` / `z42 clean <file>.z42` / 自动按时回收）：**User 2026-09-16 裁决本次不做**，
  先把基础跑通。已设计的方案（机会式 24h 触发 + 7 天未用淘汰 + 源文件消失即删 + 512 条封顶）
  登记为 Deferred 条目 `launcher-future-single-file-cache-gc`。
  规模实测：一个条目 20 KB（`dist/` 8K + 增量缓存 12K，5 行脚本），且条目数 = 跑过的**不同文件路径数**
  而非运行次数 ⇒ 短期不构成问题。
- **`z42 run` 之外的单文件命令**（`z42 test hello.z42` / `z42 build hello.z42`）：本次不做。
- **手册第 4 章起**：按 OUTLINE 后续推进。
- **文档三书重构**：已裁决但单开 change（见 memory `z42-docs-three-books`），本 change 不动目录结构。

## Open Questions

- [ ] D2（诊断路径相对化）的边界：源文件不在 cwd 之下时保持绝对路径 —— 需 User 确认这是期望行为
- [ ] 第 3 章标题与覆盖面：OUTLINE 原定「工程与构建」含 `z42.toml` 各字段 / `build --release` / 产物 /
      `clean` / 多源文件；单文件运行这一条移到第 2 章后，第 3 章是否再补「什么时候该从单文件升级到工程」一节
