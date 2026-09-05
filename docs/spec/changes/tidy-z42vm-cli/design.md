# Design: 整理 z42vm 的命令行

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/runtime-cli/spec.md](specs/runtime-cli/spec.md)

## Decisions

### Decision 1：帮助文本是**用户**文档，内部历史归代码注释

现状里 `--info` 的帮助以「docs/review.md Part 4 D5 (2026-05-25)」结尾，`--strict-config`
以「complete-runtime-settings P1 (2026-09-05) — the CI config-drift gate」结尾。这些是
**维护者要的溯源信息**，对着 `--help` 的人不需要，还挤掉了真正该说的内容。

规则：`///` doc comment（= clap help）只写「它做什么、什么时候用」；change 名 / 日期 /
spec 路径移到紧邻的 `//` 普通注释里。信息一条不丢，只是换了读者。

### Decision 2：不做子命令

`z42vm config show` 这类形态读起来更整齐，但 z42vm 的主用法是 `z42vm <file>`——
位置参数与子命令天然歧义（`z42vm info` 是跑一个叫 `info` 的文件，还是自省子命令？）。
clap 能表达，但用户脑子里那条歧义消不掉。

保持 flag 形态，用**分组 + 说清区别**解决「乱」，而不是换语法。

### Decision 3：`--stats [FORMAT]` —— 一件事一个 flag

`--print-stats-on-exit` 与 `--stats-format` 是「开关 + 它的修饰符」，而修饰符单独出现
时无意义（帮助里就写着「No effect without --print-stats-on-exit」——这是设计在道歉）。

合成一个带可选值的 flag：

    --stats           → 文本
    --stats=json      → JSON

clap 的 `num_args = 0..=1` + `default_missing_value = "text"` 正好表达「给了 flag 但没给
值 = 默认值」。

**不留别名**（pre-1.0 不做兼容）。仓内 4 处调用同步改；它没有外部消费方
（`xtask profile` 是唯一使用者）。

### Decision 4：修饰符用错地方**报错**

`--all` / `--json` 只对 `--list-knobs` / `--show-config` 有意义。今天 `z42vm --json app.zpkg`
静默无视 `--json`——用户以为自己拿到了 JSON。

这与本系统在旋钮那边的既定原则一致：**未知/无效的输入要说出来**（`--set` 未知 key 直接
exit 2）。CLI 是「此刻手敲」的层，静默忽略是最坏的。

### Decision 5：`[ENTRY]` 位置参数保留，只改帮助

它确实是 footgun：`z42vm app.zpkg foo` 会把 `foo` 当入口函数名，拼错就得到一个
「entry not found」而不是「你多打了个参数」。

但 **z42-test-runner 依赖 `z42vm <file> <test_method>` 这个形态**（每个 `[Test]` fork 一次）。
改成 `--entry` 要同步改测试运行器与它的调用面，超出本 change 的范围。这里只把帮助文本
写清楚（谁在用、什么时候需要它），把改造记为独立项。

## Implementation Notes

- clap 的 `help_heading` 加在字段上；同一 heading 的字段要**相邻**，否则分组会被打散。
- `--stats` 用 `Option<StatsFormat>` + `num_args = 0..=1` + `default_missing_value`；
  消费处从 `cli.print_stats_on_exit` / `cli.stats_format` 两个字段变成 `cli.stats`。
- 修饰符校验放在 clap 解析之后、配置装配之前（`--json` 的错误不该等到配置解析完才报）。

## Testing Strategy

| 层 | 测试 |
|---|---|
| 帮助文本 | 不含 `docs/`、不含 change 名、不含日期（一条断言扫全部 help 字符串）|
| 分组 | `--help` 输出含四个 heading |
| `--stats` | 无值 = text；`--stats=json` = json；两者都能跑通 `xtask profile` 的路径 |
| 修饰符 | `--json` / `--all` 不配合查询命令 → exit 2 + 明确消息；配合时正常 |
| 非破坏 | `--info` 首行仍是 `z42vm X.Y.Z`（`xtask_exec_profile` 唯一依赖的格式）|
