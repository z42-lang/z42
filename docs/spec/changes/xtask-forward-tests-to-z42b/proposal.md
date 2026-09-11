# Proposal: xtask 的 stdlib 测试路径转发 z42b（实测快 2.76×）

> 状态：**规划**（未实施）。前置 [`z42b-owns-test-targets`](../../archive/) 刀一已合（#578）。

## Why —— 两个实测

### ① 覆盖面完全一致，且**不需要改任何 manifest**

`z42b test src/libraries/z42.collections/z42.collections.z42.toml` 直接发现并跑通 **6 个目标 / 46 个测试**，
与 xtask 旧路径的「all 6 file(s) passed」**逐个对应**。

关键前提是既有行为：[`ManifestLoader._parseTargetSection`](../../../../src/libraries/z42.project/src/ManifestLoader.z42)
在 `[tests]` 段**缺失时也返回 kind 专属默认 include** ⇒ 25 个 stdlib 包一个字都不用改。
（子目录形态如 `tests/dict/` 是 golden 用例，归 e2e stage，不在此列 —— 两边都不算。）

### ② 快 2.76×（**订正**，原写 5.6× 是条件没对齐的错误基准）

> ⚠️ **2026-09-12 订正**：最初记在这里的「5.6×（32.63s vs 5.81s）」把**冷跑的 xtask（含整个
> 构建波）**和**热跑的 z42b** 放在一起比，条件没对齐，结论不成立。加上 `--no-build`、同为热跑
> 重测后，**xtask 反而更快** —— 因为 xtask 有 8 路并行而 z42b 的目标是串行的：
>
> ```
> z42.collections（6 单元）   xtask --no-build  3.85s   |  z42b 串行   5.88s
> z42.io        （51 单元）   xtask --no-build 31.07s   |  z42b 串行  79.01s
> ```
>
> 真正的收益在**单单元成本**：z42b 的 in-process 编译省掉「合成清单 + fork `z42c build`」的
> 往返，单单元约快 3 倍。把并行补回来之后才兑现得出来：
>
> ```
> z42.io（51 单元，同为 8 路并行）   xtask 31.07s   →   z42b --name 11.25s    （2.76×）
> ```
> （`user` 时间 78.85s ≈ 串行总量 ⇒ 父包缓存有效，并行没有重复劳动。）

根因：xtask 给**每个单元** fork 一次 `z42c build`，每次都从头 bootstrap `z42.core`
（`xtask_test_lib_units.z42` 自己的注释称之为「the dominant per-unit z42.core bootstrap」，
靠并行批次掩盖）。z42b 复用**一个进程内编译器**，父包只建一次、各目标共用。

⇒ `stdlib [Test]` stage（GREEN gate 里最慢的一段，1m14s / 占 38%）约可降到 27s 量级。

### ③ 并行留在 xtask 侧（架构裁决，2026-09-12）

**z42b 不能自己并行**：并行测试执行必须分进程（单 VM 内 `ModuleLoader` 按命名空间去重、
first-wins），而 z42b **定位不到自己** —— `__env_args` 只返回 `--` 之后的程序参数，拿不到
z42vm 路径与自身 zpkg 路径。让它能自呼需要新 VM builtin（vm 类变更，要走完整规范流程）。

**xtask 知道这两个路径**（它本来就是这么拉起 z42b 的）。所以并行仍归 xtask，只把每个单元的
实现从「合成清单 + `z42c build` + 跑产物」换成 `z42b test <toml> --name <unit>`。
**发现规则同时收归 z42b 独一份** —— 两套规则各自漂移正是 #580 查出那三处分歧的来源；
xtask 用新增的 `z42b test --list` 拿单元清单，不再自带发现规则。

**另否掉「把并行移到库级」**（每库一个 z42b 进程、库间并行）：`z42.io` 单库 51 单元 ≈ 50s，
一个库就是整条 stage 的下界，比现状 75s 好得有限。

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

- [x] **孤儿源守卫落 z42b**（2026-09-12 裁决）。理由：转发之后 xtask 不再逐 lib 枚举，守卫留在
      那里就没有东西可守；而 z42b 本就是 `[tests]` / `[bench]` 段的**所有者**，「按你自己的配置
      一个单元都没发现，但配置指向的目录里躺着源」是它对**自己配置**的一致性检查，不是语言规则。

      为什么这条守卫非有不可：z42b 在零目标时会**回落到「编译本包自己再跑」**
      （`builder_test.z42:111`）—— 于是「有源却零单元」会报出「0 passed, 0 failed」的**绿色**，
      正是 #571 要根除的「没跑 == 全过」。

      **两条豁免**（都不是硬编码名单）：
      · **子目录自带 `*.z42.toml` ⇒ 它是独立工程，不是本包的孤儿源**。这不是新发明的规则：
        编译器测试单元的发现条件就是它（`xtask_compiler.z42:242`，无 toml 即 `continue`）。
        `z42c.semantics/tests` 的 19 个、`z42c.pipeline/tests` 的 4 个全走这条。
      · **`[tests] auto = false` ⇒ 本包显式声明「这里没有约定单元」**。欠债由此记在**债主自己的
        清单**里（可见、可 grep），而不是工具里一张硬编码名单（xtask 今天的 `_isKnownOrphanLib`）。
        豁免**不等于消音**：有源时照样每轮响亮打印一行 ⚠（沿用 `test lines` 棘轮的形态）。

      顺带查出第三处分歧：**z42b 今天完全忽略 `section.Auto`**（xtask 认，见
      `xtask_manifest_targets.z42:48-53`）—— `auto = false` 在 z42b 侧是个哑配置。C1 一并对齐。
- [ ] 并行粒度：z42b 内部并行建目标，还是 xtask 并行调多个 z42b（每 lib 一个进程）？
