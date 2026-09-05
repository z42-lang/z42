# Design: launcher 转发 `--set`

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)

## Decisions

### Decision 1：launcher 只透传，不解析、不校验

`--set` 的 key 合法性 / 可用性 / 类型 / 诊断**全部归 VM**——旋钮登记表
（`KNOWN_KNOBS`）在那边，launcher 若自己校验就成了第二处 SoT，两处必然漂移。

launcher 的职责只有一条：**认出这个 flag 属于 z42vm 而不是它自己或程序**，原样传下去。
value 里含 `=`（`--set path=/a=b:/c`）也不需要它关心——切分在 `config/cli.rs` 按第一个
`=` 做。

这与既有的 `--config` / `--mode` 转发同构：launcher 认出、转发，语义归 VM。

### Decision 2：未识别的 `--flag` 报错，而不是当成 app 路径

`_cmdRun` 的扫描循环是"认识的 flag 跳过，第一个不认识的 token 当 app 路径"。于是
`z42 run --bogus app.zpkg` 会去找一个叫 `--bogus` 的工程，`z42 run app.zpkg --bogus`
则静默丢弃 `--bogus`。

改成：**app 路径确定之前**遇到 `--` 开头且不在识别列表里的 token → 报错 + 列出已知 flag。

- `--` 分隔符本身仍是"后面全给程序"，不受影响。
- app 路径**之后**的未知 flag 同样报错——它们今天被静默吞掉，是同一个坑的另一半。
- 程序自己的 flag 走 `--` 之后传，那是既有约定，不受影响。

**为什么值得一并做**：`--set` 漏转发之所以表现得那么糊涂（一种写法报"找不到工程"、
另一种写法什么都不说），根源就是这条"不认识就当路径/就丢弃"的兜底。只加 `--set`
而不修兜底，下一个漏转发的 flag 会以完全一样的方式再坑一次。

### Decision 3：`--set` 可重复，用累积扫描而非 `_flagValue`

`_flagValue(arr, flag)` 只取**第一个**匹配值——`--set` 要可重复，故单独用一次遍历收集
所有 `--set` 的值。收集与"跳过 flag+value"在同一个循环里做，避免两遍扫描的漂移。

### Decision 4：REPL 同样转发

`z42 repl --set log=z42=debug` 应当生效——REPL 是 z42vm 跑 z42i，旋钮对它一样适用。
`launcher_cli.z42` 的 `_forwardRepl` 已经在做 `--config` / `--mode` 的
「提取 + 从 z42i 参数里剥离」，`--set` 照同一模式加。

## Implementation Notes

- `launcher.z42` 现 498 行（软限 500 以下）；新增约 20 行会越过软限但远低于硬限 886
  （#455 放宽后）。软限只出 advisory、不变红。
- 已知 flag 列表在 `_cmdRun` 与 usage 文案里各有一份 —— 用一个 `_runKnownFlags()`
  返回数组，两处共用，避免加 flag 时漏改文案。
- `_stripFlag` 只剥一次（取第一个），REPL 侧收集 `--set` 同样要循环剥净。

## Testing Strategy

| 层 | 测试 |
|---|---|
| 转发 | `z42 run --set gc-mode=concurrent <app>` → 程序里 `RuntimeConfig.Source("gc-mode") == "cli"` |
| 可重复 | 两个 `--set` 都到达 |
| 值含 `=` | `--set log=z42::jit=debug` 原样到达 |
| 位置无关 | flag 在 app 之前 / 之后都行 |
| 未知 flag | `z42 run --bogus <app>` → 报错列出已知 flag，**NOT** 去找叫 `--bogus` 的工程 |
| `--` 之后 | `z42 run <app> -- --bogus` → `--bogus` 是**程序的**参数，不报错 |
| REPL | `z42 repl --set log=... -c '1+2'` 仍正常求值 |
| e2e | launcher dist smoke 加一条 |
