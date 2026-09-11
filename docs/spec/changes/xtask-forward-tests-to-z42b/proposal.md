# Proposal: xtask 的 stdlib 测试路径转发 z42b（实测快 5.6×）

> 状态：**规划**（未实施）。前置 [`z42b-owns-test-targets`](../../archive/) 刀一已合（#578）。

## Why —— 两个实测

### ① 覆盖面完全一致，且**不需要改任何 manifest**

`z42b test src/libraries/z42.collections/z42.collections.z42.toml` 直接发现并跑通 **6 个目标 / 46 个测试**，
与 xtask 旧路径的「all 6 file(s) passed」**逐个对应**。

关键前提是既有行为：[`ManifestLoader._parseTargetSection`](../../../../src/libraries/z42.project/src/ManifestLoader.z42)
在 `[tests]` 段**缺失时也返回 kind 专属默认 include** ⇒ 25 个 stdlib 包一个字都不用改。
（子目录形态如 `tests/dict/` 是 golden 用例，归 e2e stage，不在此列 —— 两边都不算。）

### ② 快 5.6×

```
xtask test stdlib z42.collections   real 32.63s
z42b test <同一个 manifest>          real  5.81s
```

根因：xtask 给**每个单元** fork 一次 `z42c build`，每次都从头 bootstrap `z42.core`
（`xtask_test_lib_units.z42` 自己的注释称之为「the dominant per-unit z42.core bootstrap」，
靠并行批次掩盖）。z42b 复用**一个进程内编译器**，父包只建一次、各目标共用。

⇒ 整个 `stdlib [Test]` stage（当前 GREEN gate 里最慢的一段，1m14s / 占 38%）有数量级级别的改善空间。

## ⚠️ 但直接替换会丢掉 `_runLibKind` 现在承担的东西

[`xtask_manifest_targets.z42:415`](../../../../scripts/test/xtask_manifest_targets.z42) 的 `_runLibKind`
不只是「编译 + 跑」：

| 职责 | 现状 | z42b 侧 |
|---|---|---|
| **孤儿源守卫** + 已知欠债棘轮 | `<subdir>/` 有 `.z42` 源却零单元 → 红（`z42.ir`/`z42c.core`/`z42c.syntax` 曾静默数月）| ❌ 无（#571 的零测试判红只覆盖「编出来没测试」，不覆盖「源根本没被认领」）|
| `--filter` | 按名过滤单元 | ❌ 只有 `--name` 精确匹配 |
| 并行（`--jobs`） | 分批 fork | ❌ 目标串行 |
| bench `--json` 聚合 | `MicroBenchAgg` → schema-v2 基线 | ❌ 只有单模块 `--format json` |
| 显式目标校验 | `_validateRunTargets` | ❌ |

**一次性替换 = 一次性丢掉上面五条**，而其中「孤儿源守卫」正是为一次真实的静默事故加的。

## What Changes —— 分四步，每步独立可验

| 步 | 内容 | 风险 |
|---|---|---|
| **A** | z42b 加 `--filter`（子串）+ **并行建目标** | 低，新增能力 |
| **B** | z42b 的 bench 聚合：多目标 `--format json` 合并输出，xtask 侧 `MicroBenchAgg` 改消费它 | 中，动基线格式消费方 |
| **C** | **孤儿源守卫搬进 z42b**（或保留 xtask 侧，仅前置扫描）—— 这条**必须先于 D** | 中 |
| **D** | `_runLibKind` 改为转发 `z42b test <lib manifest>`，旧合成路径退休 | 高：动 GREEN gate 最关键的 stage |

**D 之前不删任何旧路径**；A–C 落地后 D 才是纯切换。

## Out of Scope

- `harness=false` 目标：其 Main 会 `Environment.Exit()`，**在 z42b 进程内跑会杀掉 z42b** ⇒ 必须起子进程。独立一项。
- `[Test]` 越界判错：已证伪（见下），需 publish-ness 轴，另议。

## 已证伪的相邻设计（别再走一遍）

**「非 dev target 里出现 `[Test]` 就判错」（E0463）实现并实测后否决**：
`compile-then-test` fixture 与 xtask 合成的测试包都是**整个包就是测试工程**、无 `[[test]]` 段的合法形态，
被误杀。**「不是 dev target」≠「不该含测试」** —— 真正的判据是「产物会不会被发布」（publish-ness），
不是 dev-target 身份。同理这也否掉了 #574 的目录扫描思路。

## Open Questions

- [ ] 孤儿源守卫落 z42b 还是留 xtask？落 z42b 能覆盖所有 z42 项目，但那是「项目约定」而非「语言规则」。
- [ ] 并行粒度：z42b 内部并行建目标，还是 xtask 并行调多个 z42b（每 lib 一个进程）？
