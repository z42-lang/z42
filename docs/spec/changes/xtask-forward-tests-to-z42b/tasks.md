# Tasks: xtask 测试路径转发 z42b

> 状态：🟡 步骤 A 进行中 | 创建：2026-09-12
> 规划：[proposal.md](proposal.md)（含 5.6× 实测与「直接替换会丢掉什么」清单）

## 步骤 A —— z42b 的选择与并行

- [x] A1 `--filter <substr>`：子串筛一批。与 `--name` 分开语义 ——
      `--name` 零命中**报错并列出可选目标**（点名点不中是错），`--filter` 零命中**提示并返 0**（筛不到是正常）
- [ ] A2 并行建目标（粒度待定，见 proposal Open Questions）

## 步骤 B —— bench 聚合

- [ ] B1 z42b 多目标 `--format json` 合并输出
- [ ] B2 xtask 侧 `MicroBenchAgg` 改消费它（schema-v2 基线格式不变）

## 步骤 C —— 孤儿源守卫（**必须先于 D**）

- [ ] C1 「`<subdir>/` 有 `.z42` 源却零单元」的守卫 + 已知欠债棘轮，搬进 z42b 或前置扫描保留在 xtask
      —— 这条是为一次真实静默事故（`z42.ir`/`z42c.core`/`z42c.syntax` 静默数月）加的，不能在转发中丢

## 步骤 D —— 切换（高风险，动 GREEN gate 最关键的 stage）

- [ ] D1 `_runLibKind` 改为转发 `z42b test <lib manifest>`
- [ ] D2 旧合成路径（`_renderSyntheticManifest` 等）退休
- [ ] D3 前后耗时对账（当前 `stdlib [Test]` 1m14s / 占 gate 38%）

## 备注

**A1 实测**：
```
z42b test <collections> --filter stack  → 只跑 stack，5 passed
z42b test <collections> --filter zzz    → "no test target matched --filter `zzz`"，rc=0
z42b test <collections> --name  zzz     → "no test target named `zzz`" + available: linkedlist, list_api, …
```
GREEN：`xtask test` 全 13 stage 绿。
