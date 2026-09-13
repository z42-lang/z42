# Proposal: `z42 run` / `z42 repl` 转发 `--set`（launcher-forwards-set）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`toolchain`（+ `docs` 补一页遗漏的 stdlib 页）
> 前置：[`complete-runtime-settings`](../complete-runtime-settings/)（#443，`--set` 落地）

---

## Why

`--set <key>=<value>` 是优先级链的**最高层**，也是 `complete-runtime-settings` 的核心交付。
但它只在**直接调 `z42vm`** 时可用——**主前门 `z42 run` 传不进去**，而且两种写法各错各的：

| 命令 | 现状 |
|---|---|
| `z42 run --set gc-mode=x app.zpkg` | `--set` 不在 `_cmdRun` 的识别列表里 → 落到 `if (app.Length == 0) { app = a; }` → **`--set` 被当成 app 路径**，随后"找不到工程/文件"报错 |
| `z42 run app.zpkg --set gc-mode=x` | app 已定，后续 token 既不转发也不报错 → **静默丢弃** |
| `z42 repl --set gc-mode=x` | 同样不识别，混进 z42i 的参数里 |

launcher 已经在转发 `--config`（→ `Z42_CONFIG`）与 `--mode`（→ z42vm `--mode`），
`--set` 是同一族里唯一漏掉的——而且是最常用的那个。

**顺带暴露一个更一般的坑**：`_cmdRun` 对**任何**不认识的 `--flag` 都是"当成 app 路径或
静默跳过"。`z42 run --bogus app.zpkg` 今天会去找一个叫 `--bogus` 的工程。这与 VM 侧
`--set` 未知 key 就 exit 2 的"不猜"原则相反。

## 顺带补一处文档欠账

`complete-runtime-settings` 的 tasks.md 把 `docs/book/src/stdlib/…（Std.Runtime.RuntimeConfig
表面）` 勾成了完成，**但那一页从没写过**——收尾时用正则把所有 `- [ ]` 一律翻成 `- [x]`，
没有逐项核对。本 change 补上，并把那条勾回 `- [ ]` 再随本 change 完成。

---

## What

### A. `--set` 进 launcher 的 flag 族（可重复）

`z42 run` 与 `z42 repl` 各自识别 `--set k=v`（可出现多次），原样转发给 z42vm。
value 里含 `=` 不需要 launcher 关心——它只做透传，切分归 VM（`config/cli.rs` 按第一个
`=` 切）。

### B. 未识别的 `--flag` 明确报错，不再当成 app 路径

`z42 run` 在 app 路径确定**之前**遇到未知的 `--`-前缀 token → 报错列出已知 flag。
`--` 分隔符之后的一切仍原样透传给程序（不变）。

### C. 补 `docs/book/src/stdlib/runtime-config.md`

`Std.Runtime.RuntimeConfig` 的表面页：六个方法的语义、扁平 `string[]` 形态的切分约定、
为什么没有 setter、与 `Environment.GetEnvironmentVariable` 的区别。

---

## What This Does NOT Do

- **不在 launcher 里校验 key**：`--set` 的 key 合法性、可用性、诊断全归 VM（登记表在那边，
  launcher 复制一份就是第二处 SoT）。launcher 只透传。
- **不给 `z42 build` / `z42 publish` 加 `--set`**：那些是构建期命令，运行时旋钮与它们无关
  （工程要固化运行时设置走 `[profile.*]`）。
- 不动 `--config` / `--mode` 的现有语义。

## 阶段

| 阶段 | 内容 | 风险 |
|---|---|---|
| **P0** | `z42 run` / `z42 repl` 转发 `--set` + usage 文案 | 低 |
| **P1** | 未识别 `--flag` 报错 | 低（行为收紧，但当前行为是"找一个叫 --bogus 的工程"，不可能有人依赖）|
| **P2** | 补 stdlib 文档页 + 把 tasks.md 那条勾回真实状态 | 低 |

## Scope
`src/toolchain/launcher/core/launcher.z42` · `launcher_cli.z42` ·
`docs/book/src/stdlib/runtime-config.md`(NEW) · `docs/book/src/SUMMARY.md` ·
`docs/spec/changes/complete-runtime-settings/tasks.md`(勾回)

## 未决
无。
