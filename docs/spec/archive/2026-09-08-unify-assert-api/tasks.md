# Tasks: 合并两份 Assert

> 状态：🟢 已完成 | 完成：2026-09-08
>
> 归属：[[restore-emit-zbc-diagnostics-program]] 的前置项（开 `--emit-zbc` 的门之前先消掉
> 216 条 E0401 的起因）。方向已由 User 在会话中确认。

## 进度概览

- [x] 阶段 0：量测 —— 全仓同短名类扫描（`Assert` 是唯一一例）、600 个调用点的 using 组合分布、
      非测试代码是否真用 `Assert.`（答案：没有）
- [x] 阶段 1：合并本体
- [x] 阶段 2：善后（golden / 注释 / README / design 文档）
- [x] 阶段 3：GREEN + 自举不动点 + PR

## 阶段 1：合并本体

- [x] 删 z42.core 原来那份 8 方法的 `Assert.z42`
- [x] `z42.test/src/Assert.z42` **移到** `z42.core/src/Assert.z42`，`namespace Std.Test;` →
      `namespace Std;`，抬头改写（合并理由 + 「合并 ≠ 修好那条 binder/emitter 不对称」的告诫）
- [x] `Failure.z42` **一并移到 z42.core**（namespace `Std` 不变 ⇒ FQN 不变 ⇒ runner 分类不受影响）
- [x] `TestRunner.Fail`：`e.Message` → `e.ToString()`（否则失败输出从
      `AssertionError: expected 1 but got 2` 降级成 `values not equal`）

### 🔴 实施期实测推翻的一版设计（务必留档）

初版按「唯一那份留在 z42.test」做（理由：全仓非测试代码没一处真调 `Assert.`，断言 API 不必进
prelude）。`xtask test` 当场否掉：**183 条 golden 全红 `VCall: expected object, got Null`**。

根因 = `ImportedSymbolLoader._isPrelude(pkg) { return pkg == "z42.core"; }` —— **prelude 是包粒度**，
非 prelude 包要 `using` 命中其 ns 才激活；而 `src/tests/` 有 **188 个 golden 一行 `using` 都不写**。
⇒ `Assert` 必须住 z42.core，它抛的 `TestFailure`/`SkipSignal` 只能跟着进来。

**教训**：判断「某个类能不能搬出 prelude 包」，要看 **prelude 的判定粒度是包还是命名空间**，
不能只看「谁在调用它」。

## 阶段 2：善后

- [x] `z42.test/tests/test_runner/expected_output.txt` —— FAIL 行随 `ToString()` 刷新
- [x] `z42.core/tests/std_assert/` —— 抬头理由（「写成框架单元会被 `using Std.Test` 抢走裸名」）
      已失效。改写抬头、保留用例：它走的是「零 `using` 纯靠 prelude 拿到 `Assert`」这条路径，
      而 `src/tests/` 下另有 188 个同款 golden 依赖它（那条路径的重要性正是被本次实测量出来的）。
      ⚠️ 该文件（含注释）仍**不得出现测试框架的属性字面量**（`_dirHasTestMethods` 是朴素全文
      子串匹配、连注释也算 —— 见 [[audit-silent-gates-program]]）。
- [x] `z42.core/tests/assert_basics.z42` 抬头（「测的是 Std.Assert 而非 Std.Test.Assert」已无意义）
- [x] `z42.test/tests/failure_location_demo.z42:24` —— 那段「Assert.Equal 会 first-wins 到
      z42.core 的裸 Exception，所以这里 catch Exception」的推理已失效。
      **代码不用改**（`TestFailure : Exception`，catch 照样命中），只改注释。
- [x] `z42.test/tests/assert_collection_helpers.z42:95` 注释
- [x] `z42.core/README.md` 行 19-20（两种测试形态的选择理由里引了这个碰撞）
- [x] `z42.test/README.md` 行 137/143
- [x] `z42.ir/src/DependencyIndex.z42:53` + `z42c.semantics/src/CuPreprocess.z42:109-111`
      —— 两处注释举的例子（`Std.Assert` vs `Std.Test.Assert`）已不存在。
      **机制本身保留**（对任意同短名对仍有效），只改举例。
- [x] `.claude/rules/common-pitfalls.md` §1 —— 现场案例正是这一对。补一句「起因已于
      unify-assert-api 消除；规则本身不变（任何 first-wins 注册仍需显式排序）」
- [x] `docs/design/testing/{README,testing,test-runner-bootstrap}.md`、
      `docs/design/language/object-protocol.md`、`src/tests/README.md` 里的
      `Std.Test.Assert` / 「两份 Assert」表述

## 阶段 3：验证

- [x] `xtask build stdlib` + `build compiler` 无 `✗`（grep 过）
- [x] `xtask test` → **✅ GREEN — all stages passed**（2m40s）
- [x] `xtask test stdlib --mode jit` → **all 331 file(s) passed (in 23 lib(s))**
- [x] 自举字节不动点 → **3/3 packages gen1==gen2 (--workspace)**
- [x] PR

## 已回答的待验项

- **dir-mode 单元能否看到 z42.test？** —— 问题随方案变更作废：`Assert` 最终落在 z42.core
  （prelude 包），dir-mode 单元本就恒可见。`std_assert/` 全绿。
- **`Assert` 能不能搬出 prelude 包？** —— **不能**。见上「实施期实测推翻的一版设计」。
