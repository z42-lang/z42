# Tasks: tidy-z42vm-cli

> 状态：🔴 DRAFT | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-cli/spec.md)

## P0 — 帮助文本与分组
- [ ] 0.1 每个 flag 的 `///` 只留用户向说明；change 名 / 日期 / spec 路径移进 `//` 注释
- [ ] 0.2 `help_heading` 四组，同组字段相邻
- [ ] 0.3 三个自省命令各自写清区别
- [ ] 0.4 `[FILE]` / `[ENTRY]` / `[ARGS]` 的说明收短

## P1 — `--stats [FORMAT]`
- [ ] 1.1 合并两个 flag；`num_args = 0..=1` + `default_missing_value = "text"`
- [ ] 1.2 消费处改读单字段
- [ ] 1.3 `scripts/xtask_profile.z42` 4 处调用同步

## P2 — 修饰符校验
- [ ] 2.1 `--all` / `--json` 未配合查询命令 → exit 2 + 明确消息

## GREEN
- [ ] help 文本断言（无 `docs/` / 无 change 名 / 有四个 heading）
- [ ] `--info` 首行仍为 `z42vm X.Y.Z`
- [ ] 全套门禁

## 未决
无。
