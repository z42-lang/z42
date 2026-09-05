# Tasks: 修 xtask 文档/命令面漂移 + 加 stage 清单防漂移门

> 状态：🟢 已完成 | 创建：2026-09-05 | 完成：2026-09-05

**变更说明：** 修 `scripts/` 与其文档之间已发生的四类漂移（GREEN gate stage 清单、README
结构树、过期头注释、指向不存在命令的用户可见字符串），并加一道**会变红的门**锁住 stage 清单，
使这类漂移不再复发。

**原因：** [`docs/book/src/dev/test-gate.md`](../../../book/src/dev/test-gate.md) 自称
「GREEN gate stage 组成的唯一权威清单（SoT）」并明文警告「历史上复列 5-6 处、已互相漂移」，
但它自己已经烂了——列 5 个 stage，`_testAll` 实际跑 8 个。缺的三个（multi-exe / manifest
targets / examples）分别由 unify-run-modes P3、add-tests-bench-manifest-config P4、设计 D4
引入，三次都没同步这页。**纯靠纪律的 SoT 已被证明守不住**，所以本次除了修数据，还要补一道
机械门（形态照抄已有的 `test vscode-syntax` grammar↔Lexer 防漂移门）。

**变更类型：** `fix` / `docs`（不改任何命令语义与执行行为；新增的只是一个一致性断言）

**文档影响：** `docs/book/src/dev/test-gate.md`（stage 清单 + 新门说明）、
`scripts/README.md`（源码结构树 + test 流程图）。

---

## 阶段 1: 防漂移门（先做——它定义了 stage 清单的机器可读形态，后面的文档按它写）

- [x] 1.1 `scripts/test/xtask_test.z42`：加 `_gateStageNames()` 返回 GREEN gate 的规范 stage
      名单（唯一代码侧 SoT）—— **实为 10 条**（9 个验证 stage + build wave，后者也走 banner/计时）
- [x] 1.2 注册断言直接加进**既有**的 `_stageStart(name)`（`_isRegisteredStage`）——
      原计划新加 `_stageBanner`，但 origin/main 已有 `_stageStart` 统一打 banner，复用即可，
      不新增重复入口
- [x] 1.3 ~~改走 `_stageBanner`~~ —— 无需改：10 处 stage 打印本就已走 `_stageStart`
- [x] 1.4 加 `_checkGateStageDoc()` —— 读 test-gate.md 的 `<!-- gate-stages:begin/end -->` 区
      逐条比对；放在 `_testAll` **构建波之前**（原计划放之后，改前置：文档漂移不该让人等
      十几分钟构建波跑完才发现）
- [x] 1.5 反向验证：把文档里 `e2e multi-exe` 改成 `-TYPO` → gate 秒退 exit 1 并打印逐条差异；
      已还原

## 阶段 2: 文档数据修正

- [x] 2.1 `docs/book/src/dev/test-gate.md`：mermaid **6 → 9** 个验证 stage 节点（补 multi-exe /
      manifest targets / examples）；「依序跑六个」→「九个」；加机器可读
      `<!-- gate-stages:begin/end -->` 清单区 + 新门说明；决策表补一行；实现表「四 stage 串联」
      → 实况
- [x] 2.2 页头「对齐」日期 2026-07-07 → 2026-09-05
- [x] 2.3 `scripts/README.md`：**删掉**重复的 stage 清单（改为指向 test-gate.md）——按该页
      「其他文档不再各自复列」的约定，修重复源比同步重复源更对；源码结构树补齐 20 个漏列
      文件 + 标注三处名实不符；另 3 处「e2e + stdlib + compiler」式的不完整 gate 描述改为指向 SoT

## 阶段 3: 过期注释与用户可见字符串

- [x] 3.1 `scripts/xtask.z42` 头注释：命令面重写为现行（现列的 `build launcher` / `test vm` /
      `test cross-zpkg` 均已不存在），删「MVP Stream 3 委托给现有 .sh」的过期叙事
- [x] 3.2 `scripts/common/xtask_common.z42` `_z42cMode` 上方两段互相矛盾的注释（前段称默认
      interp、后段称默认 jit；代码是 jit）合并为一段
- [x] 3.3 `scripts/package/xtask_test_{stage_components,package_assemble}.z42` 头注释里
      「wired into `test`」的假声称改为实情（`test packages` 是 opt-in，不在 `test all` 内）
- [x] 3.4 **死命令**（无歧义 bug）：`build package` 这个子命令不存在（现为 `package sdk` /
      `package runtime --rid`），却出现在 3 处——`xtask_test_dist.z42:46` 的报错提示、
      `xtask_package_ios.z42:106` **烤进生成的 Package.swift 头注释**（会随发行包发出去）、
      `xtask_package.z42:121` 的报错前缀
- [x] 3.5 `scripts/` 内**用户可见的「run: X」类提示**改用现行推荐形式 `xtask <cmd>`（7 处）

> **3.5 的边界（重要）**：`z42 xtask.zpkg <cmd>` 走 launcher **仍然有效、不是错的**，只是
> 不再是推荐形式（原生 apphost `./xtask` 更 ergonomic）。既有 memory
> `feedback_xtask_apphost_direct_run` 明确记着「既有大量 docs 仍写 `z42 xtask.zpkg`（非错）；
> **是否全量 sweep 待 User 定**」——故本 PR **只改 `scripts/` 里叫用户去敲命令的报错提示**
> （敲了就该能用），**不做** docs 全量 sweep。以下一律不动：
> - `docs/spec/archive/**`：历史记录，不可改写
> - `docs/spec/changes/**`（他人在飞 change）：不属本 PR
> - `docs/design/runtime/launcher.md:192`：描述的是**冷启动链路**，那时 `./xtask` 还不存在，
>   `.z42/z42 xtask.zpkg …` 正是当时唯一可用形式——**写法正确，改了反而错**

## 阶段 4: 验证与归档

- [x] 4.1 `xtask test` 完整 GREEN —— ✅ 10/10 stage 全过，3m11s
- [x] 4.2 归档到 `docs/spec/archive/2026-09-05-fix-xtask-doc-drift/` + 本文件标 🟢
- [x] 4.3 commit + 开 PR

## 验证报告

### `xtask test` 状态：✅ 全绿（10 stage）

```
build wave 43.2s · e2e goldens 16.2s · e2e cross-zpkg 2.9s · e2e multi-exe 1.2s
stdlib [Test] 1m21s · manifest targets 3.2s · examples 0.7s · compiler 40.7s
vscode-syntax 0.0s · lines 1.9s          TOTAL 3m11s
```

- `_checkGateStageDoc` 正向：gate 开跑前静默通过，直接进构建波 ✓
- `_checkGateStageDoc` 反向：把文档清单里 `e2e multi-exe` 改成 `-TYPO` → **秒退 exit 1** 并
  逐条打印「代码 10 条 vs 文档 10 条」的差异 ✓（已还原）
- `compiler` stage（自举不动点 + units 19/19）绿 → 未破自举
- `lines` stage：scanned 814 file(s)，6 known over-limit、**0 new/grown** ✓
  （顺带确认：该门只扫 `src/`，`scripts/` 确实不在尺寸纪律覆盖内）
- 文档相对链接全部可解析（README 7 条 + test-gate.md 1 条，无死链）
- 命令面机械检查：`grep -rn "build package" scripts/` **清零**

### 环境备注（不属本变更，但影响复现）

本地所有 worktree 的种子都停在 zpkg **0.41**，而 main 已是 **0.43** → 首轮 GREEN 在构建波
`✗ z42c --workspace self-build failed`（种子旧格式 stdlib 被铺进 in-tree，新格式 z42vm 加载即炸）。
按 memory `fresh-worktree-seed-setup` 的 overlay 配方解决：下载 CI `toolchain-macos-15` artifact
（run 33968581816，sha 与本分支 base 同为 `a70352ed`）overlay 进 in-tree → cargo 建 0.43 z42vm →
**用 overlay 出来的 0.43 z42c 重建 `xtask.zpkg`**（否则测的是 CI 版 xtask、自己的改动静默不生效）→
`Z42_PORTABLE_VM=<0.43 vm> ./xtask test`。该配方的两处缺口已回写 memory。

## 备注

- **Out of scope**（留给后续 PR）：golden 语料枚举四份合一（最高杠杆，独立 PR）、
  命名归位（`xtask_test_*` 三义消歧 / targets↔fixtures 正名 / `xtask.z42` 五个错位 handler）、
  超限文件拆分。本 PR 只动注释、文档、用户可见字符串与新增的一致性断言，**不移动任何代码块**。
- `scripts/` 不在 `.claude/rules/code-organization.md` 的 `paths`（`src/**` + `docs/**`）覆盖内，
  故 4 个超 500 行文件严格讲不算违规；「要不要把 scripts/ 纳入尺寸纪律」是独立议题，本 PR 不裁决。
