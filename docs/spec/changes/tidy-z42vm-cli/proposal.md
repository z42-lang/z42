# Proposal: 整理 z42vm 的命令行（tidy-z42vm-cli）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`vm`（CLI 表面）→ 完整流程
> User 反馈（2026-09-05）：「z42vm 的命令行参数有点乱不够清晰」

---

## Why

`complete-runtime-settings` 那一串 change 往 z42vm 上加了 6 个 flag，加完没回头整理。
现在 `--help` 的实际样子：

```
      --info    Print runtime build info (version / target / build profile / enabled
                features / exec modes / libs dir / Z42_PATH) and exit. Useful for bug
                reports and CI preflight. docs/review.md Part 4 D5 (2026-05-25)
      --strict-config    ... complete-runtime-settings P1 (2026-09-05) — the CI
                config-drift gate
```

四个具体问题：

1. **帮助文本里塞满了内部历史**——change 名、日期、`docs/review.md Part 4 D5` 这类
   spec 引用。那是给维护者的，不是给用户的；它们该在代码注释里。
2. **13 个选项平铺，没有分组**。执行控制、运行时配置、自省、统计混在一起。
3. **三个自省命令是 flag 形态，还带两个游离修饰符**（`--all` / `--json` 只对其中两个
   有意义，对 `--info` 无意义）。而且从帮助里看不出这三个有什么区别。
4. **`--print-stats-on-exit` + `--stats-format` 是「一个 flag 加它的修饰符」**——
   两个 flag 表达一件事。

第 3、4 两条还有个共同的坏行为：**修饰符用错地方是静默无效的**。
`z42vm --json app.zpkg` 什么都不说，就是没有 JSON。

---

## What

### A. 帮助文本只写给用户看

删掉所有 change 名 / 日期 / spec 路径，移进代码注释（信息不丢，只是换地方）。
每个 flag 一到两行说清「它做什么」。

### B. 按职责分组（clap `help_heading`）

```
执行:        --mode  -v/--verbose
运行时配置:   --set  --strict-config
自省:        --info  --list-knobs  --show-config  --all  --json
诊断:        --stats
```

### C. 三个自省命令的区别写进帮助

```
--info          构建信息 + 完整旋钮快照（提 bug 时贴这个）
--list-knobs    有哪些旋钮：类型 / 可设置层 / 本 build 可用性 / 默认值
--show-config   旋钮当前是什么值、来自哪一层、以及某层的值为什么没生效
```

### D. `--print-stats-on-exit` + `--stats-format` 合成 `--stats [FORMAT]`

```
--stats           退出时打印计数器（文本）
--stats=json      同上，单行 JSON（工具消费）
```

一件事一个 flag。仓内 4 处调用（`scripts/xtask_profile.z42`）同步更新。
pre-1.0 不留别名。

### E. 修饰符用错地方要报错，不静默

`--all` / `--json` 不配合 `--list-knobs` / `--show-config` 使用 → 明确报错。
与本系统在旋钮那边坚持的「不猜、说出来」一致。

---

## What This Does NOT Do

- **不改成子命令**（`z42vm config show`）：z42vm 的主用法是 `z42vm <file>`，加子命令会与
  位置参数 `[FILE]` 歧义（`z42vm info` 是跑一个叫 `info` 的文件还是子命令？）。
- **不动 `[ENTRY]` 位置参数**：它是 footgun（`z42vm app.zpkg foo` 把 foo 当入口），但
  z42-test-runner 依赖 `z42vm <file> <test_method>` 这个形态，改动面超出本 change。
  只改进它的帮助文本。
- 不改任何 flag 的语义（除 D 的合并）。

## 阶段

| 阶段 | 内容 | 风险 |
|---|---|---|
| **P0** | 帮助文本清理 + 分组 + 自省三命令的区别 | 低（纯文案 + clap 属性）|
| **P1** | `--stats [FORMAT]` 合并 + 4 处调用点 | 低 |
| **P2** | 修饰符错用报错 | 低 |

## Scope
`src/runtime/src/main.rs` · `scripts/xtask_profile.z42` · `src/runtime/src/main_tests.rs`

## 未决
无。
