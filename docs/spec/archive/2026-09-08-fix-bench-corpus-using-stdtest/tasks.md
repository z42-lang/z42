# Tasks: 修复 bench 语料 + 给它装一道会红且挡人的门

> 状态：🟢 已完成 | 创建：2026-09-08 | 完成：2026-09-08

**变更说明：** 14 个 stdlib bench 文件补 `using Std.Test;`（修 main 上 14/19 bench 文件跑不起来），
并把「stdlib [Benchmark] 语料能跑」加成 GREEN gate 的一个 stage。

**原因：** `unify-assert-api`（#532）把 `Failure.z42`（`namespace Std;`）从 z42.test 搬进 z42.core 后，
z42.test 只剩 `Std.Test` / `Std.Test.Contracts` 两个命名空间。而包激活是**整包粒度**的
（`ImportedSymbolLoader._pkgProvidesUsing`：包内**任一**模块 ns 命中调用方**任一** `using` → 整包激活、
全部类按短名注册），bench 文件此前是靠 `using Std;` 命中 `Failure.z42` 的 `namespace Std;`
**搭便车**激活整个 z42.test 才看得见 `Bencher` —— 这条依赖是巧合，不是设计。便车没了 ⇒ `Bencher`
解析为 `Z42UnknownType`（`Name()` = `"<unknown>"`）⇒ emitter 照发 `newobj Z42<X>Bench.<unknown>` ⇒
运行期空 TypeDesc ⇒ `VCall: function 'Z42<X>Bench.<unknown>.get_WarmupIters' not found`。

编译期之所以静默 exit 0，是 `--emit-zbc` 吞诊断那个洞（`restore-emit-zbc-diagnostics` 程序在修，
本变更不碰）。**根因修复落在产出端**：bench 文件本就该显式 `using Std.Test;`（它们确实在用
`Std.Test.Bencher` / `Std.Test.BenchHelpers`），5 个已有该行的 bench 文件从头到尾没坏过。

**为什么同时加 gate stage：** `bench-regression` 自 #532 起对**每个** PR 恒红，却因不在
required 列表而无人过问 —— 连红 3 个 PR。「会红但不挡人的门 = 没有门」。而把
`bench-regression` 直接提 required 不可行：它是 path-filtered workflow，纯文档 PR 上不触发 ⇒
required check 恒 pending ⇒ PR 永远合不了。正确收口是把**「语料能不能跑」**（确定性、
`bench stdlib --no-build` 本机 14.6s、只有跑挂才红、不判时间⇒零噪声）从
**「跑多快」**（`bench-regression` 的噪声判红层）里拆出来，前者进 GREEN gate。

**文档影响：** `docs/book/src/dev/test-gate.md`（gate-stages 区，防漂移门要求代码/文档两处一致）、
`docs/workflow/testing/` 对应页、`src/libraries/*/bench/` 相关 README「如何测试验证」（如涉及）。

## 任务

- [x] 1.1 14 个 bench 文件补 `using Std.Test;`（紧随 `using Std;`，与已有 5 个文件同一写法/位置）
- [x] 1.2 验证：`xtask bench stdlib` 19/19 文件全过、exit 0（修前 14/19 失败）
- [x] 1.3 `scripts/test/xtask_test.z42`：`_testAll` 加 stage `stdlib [Benchmark]`（skippable="bench"）
      + `_gateStageNames()` 同步
- [x] 1.4 `docs/book/src/dev/test-gate.md` gate-stages 区同步（防漂移门：两处一致才绿）
      + 决策表、mermaid、stage 数、实现表同步
- [x] 1.5 `.github/workflows/ci.yml`：非 linux-x64 的 test-host 腿 `--skip` 加 `bench`
      （host-independent，一条腿足够；同 cross-zpkg 的处置）
- [x] 1.6 `scripts/test/xtask_test_changed.z42`：`src/libraries/<lib>/bench/*` 从 `_mapSkip()`
      改映射到 `xtask bench stdlib <lib>`（此前改 bench 文件 = 零验证，同一族的静默门）
- [x] 1.7 文档同步：`docs/workflow/testing/changed-only.md` 映射表 + `testing/README.md`
      （删掉又一份漂了的 stage 复列，改为链 SoT）+ `book/compiler/project-model.md`
      新增「激活是整包粒度」机制节（含现场案例）+ `z42.test/README.md` 显式 `using Std.Test` 警告
- [x] 1.8 完整 GREEN（`xtask test`）—— ✅ 全绿 11 stage / 2m48s（`stdlib [Benchmark]` 12.1s）；
      另做反向验证：只改文档侧的 gate-stages 区 → `_checkGateStageDoc` 当场红并打印差异（门非空转）
- [x] 1.9 归档（本目录 → `docs/spec/archive/2026-09-08-fix-bench-corpus-using-stdtest/`）+ 随 PR 一起提交

## 备注

- 已 grep 全仓确认无第二处同形状漏网：用 `Bencher`/`BenchHelpers`/`BenchStats`/`TestIO`/
  `TestReport`/`TestRunner` 而无 `using Std.Test;` 的文件只剩 4 个，命中全在**注释**里。
- **不在本变更 Scope**：`--emit-zbc` 吞诊断（另一程序）、`bench-regression` 判红阈值/复测语义
  （`investigate-micro-ab-false-regressions`）。
