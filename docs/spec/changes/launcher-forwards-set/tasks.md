# Tasks: launcher-forwards-set

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-settings/spec.md)

## P0 — 转发 `--set`
- [x] 0.1 `_runKnownFlags()`：已知 flag 单一来源，`_cmdRun` 扫描与 usage 文案共用
- [x] 0.2 `_cmdRun`：一次遍历里收集全部 `--set` 值（可重复）并从 app 路径扫描中剥离
- [x] 0.3 转发：每个 `--set` 以 `--set <k=v>` 形式加到 z42vm 命令行
- [x] 0.4 `launcher_cli.z42` `_forwardRepl`：同款收集 + 剥离 + 转发
- [x] 0.5 usage / help 文案加 `--set`

## P1 — 未识别 flag 报错
- [x] 1.1 app 路径确定前后遇到未知 `--` 前缀 token → 报错列出已知 flag，exit 2
- [x] 1.2 `--` 分隔符之后不受影响

## P2 — 补文档欠账
- [x] 2.1 `docs/book/src/stdlib/runtime-config.md`（NEW）+ SUMMARY 挂页
- [x] 2.2 把 `complete-runtime-settings/tasks.md` 里那条误勾的 stdlib 文档项勾回并注明

## GREEN
- [x] launcher dist smoke 加一条 `--set` 断言；全套门禁

## 未决
无。


## 落地记录（2026-09-05）

**首次按 `parallel-development.md` §0 在专属 worktree 里做**（`wt-launcherset`，
基于 `origin/main`）。本轮更早的四个 change（#443/#446/#449/#458）都是在**主树**上开分支
做的，违反 §0——已向 User 指出。供种只需 14M（`.z42` + `xtask.zpkg` +
`artifacts/build/{libraries,compiler,toolchain}`），不必拷 5.5G 的全量 artifacts。

**踩到一个会重复踩的坑**：`scripts/test/*.z42` 是**编进 `artifacts/xtask/xtask.zpkg`** 的
——从主树拷来的种子 xtask 不含本 worktree 对 smoke 的修改，于是新加的两条断言"静默不跑"
（`Total: 3` 而非 5，且不报错）。必须
`z42vm <z42c.driver.zpkg> -- build scripts/xtask.z42.toml --release` 重编 xtask.zpkg 才生效。

**手工端到端**（四种情形，每条都先确认旧行为）：
① `--set` 在 app 前 → `src=cli`；② 在 app 后 + 可重复 + 值含 `=` →
`gc-mode=generational log=z42::jit=debug`；③ `--bogus` → 明确报错列出已知 flag
（此前会去找一个叫 `--bogus` 的工程）；④ `--` 之后归程序，不报错。

**GREEN**：runtime cargo 1159/0；release 无新告警（10 = 本 worktree main 基线）；
自举不动点 3/3；e2e 566 + cross-zpkg 17 + multi-exe 2；launcher dist smoke **5/5**
（新增两条）；lines 6 known / 0 new-grown。

## 顺带清掉的文档欠账
`complete-runtime-settings/tasks.md` 里 `docs/book/src/stdlib/…` 那条当初被**误勾**——
收尾时用正则把所有 `- [ ]` 一律翻成 `- [x]`，没有逐项核对，而那一页从没写过。
本 change 补上 `docs/book/src/stdlib/runtime-config.md` 并在原 tasks.md 注明了这段。
