# unify-golden-corpus-walk — golden 语料枚举四份合一

> 类型：`refactor`（最小化模式，无需 DRAFT 规范）。
> 属 [scripts/ 结构优化程序](../../../../.claude/) 第 2 批（第 1 批 = PR #472 文档漂移 + gate stage 防漂移门）。

## 问题

同一套 golden 语料的三段走遍历（`src/tests/<cat>/<name>/source.z42`、
`src/libraries/<lib>/tests/`、flat `src/tests/<cat>/<name>.z42`）被抄了 **4 遍**，
靠注释里写「mirrors XXX」的**人工镜像**同步：

| 实现 | 位置 | 产出 | 服务命令 |
|---|---|---|---|
| `_collectGoldenCasesF` | `build/xtask_test_assets.z42` (115 行) | `_GoldenCases` | `build test` |
| `_enumerateCasesF` | `test/xtask_test_vm.z42` (117 行) | `_Case` | `test e2e` |
| `_enumerateDistCases` | `test/xtask_test_dist.z42` (83 行) | `_DistCase` | `test dist` |
| `_enumerateCorpus` | `test/xtask_test_embedded_corpus.z42` (87 行) | `_CorpusCase` | `test embedded` / `test list` |

**已经漂了**（做这件事的实证理由，不是预防性重构）——类别排除谓词有三套互不相同的定义，
注释还互相声明「我跟另一个不一样」：

- `_isNonRunnableCat`（`common/xtask_golden.z42`）
- `_isNonRegenCat`（`build/xtask_test_assets.z42`）
- 内联 `cat != "errors" && cat != "parse" && !_isNonRunnableCat(cat)`（`test/xtask_test_dist.z42` flat 段）

另：`_isExcludedDirName`（两个已知失败用例）只在 vm / embedded 两路调用，assets / dist 没有。

## 调查结论（改之前先核的事实）

1. **`src/tests/errors/` 与 `src/tests/parse/` 这两个类别今天根本不存在**——它们是 C# 编译器
   移除时随 `z42.Tests/` 蒸发的负例语料（见 memory `selfhost-migration-lost-negative-tests`）。
   所以三套谓词里提到 errors/parse 的部分**全是死策略**，实际生效的只有两套：
   - regen 集（`_isNonRegenCat`）：保留 `zbc-format` / `zpkg-format`——它们**要**被重新生成为字节基线
   - runnable 集（`_isNonRunnableCat`）：排除 `zbc-format` / `zpkg-format`——没有 stdout 可比对

   这个差异是**有意的、正确的**，不是漂移。dist flat 段的内联谓词才是多余的第三份。
2. **`_isExcludedDirName` 的缺席是真差异**：`test dist` 会跑
   `gc/composite_ref_weak_mode` + `delegates/multicast_subscription_refs` 这两个
   vm / embedded 明确排除的已知失败用例（实测 DIST-interp 288 vs VM-interp 286，差的正是这两条）。
   **本 PR 不改这个行为**（纯重构、保持等价），但重构后它是调用点上的一行显式过滤，
   不再埋在 83 行遍历里——要不要统一是独立议题。
3. **`Path.Glob` 本身是 sorted 的**（`runtime/src/corelib/fs.rs` 注释与实现均确认），
   所以旧 flat 段没有 common-pitfalls §1 的非确定性隐患——**不要**把这条当作本 PR 的收益。

## 方案

`common/xtask_golden.z42` 提供**唯一**的 `_walkGoldenCorpus(root) → _GoldenEntry[]`：
policy-free，只报「找到了什么 + 每个消费者判断所需的事实」（类别、sidecar、
artifacts 镜像位置、目录里有没有 `[Test]`），**准入策略留在 4 个调用点**——四套集合本就
有意不同，把差异摆在调用点比藏在四份遍历里更可读。

段与发射顺序（沿用既有 regen / e2e / dist 顺序）：
`tests-dir` → `lib-dir` → `tests-flat` → `lib-file`；段内类别/库排序、用例名排序。

`test embedded` / `test list` 的**发射顺序是 load-bearing 的**（`_sampleCorpus` 与分片切片
依赖「同 bucket 连续」，且 src/tests 桶内 dir-mode 与 flat-mode 按原始 basename **交错**排序），
所以它在共享 walk 之上做一次按桶重组（`_entriesInBucket` 双指针归并），而非单遍扫描。

顺带删掉 3 份重复的 growable append 模板（`_appendCase` / `_appendDistCase` / `_appendCorpus`）：
消费者现在知道条目数上界，直接定长分配 + 尾部 trim。

## 验证（行为等价的硬证据）

改动是**行为敏感**的（会改变「哪些用例算数」），所以用了对账而非「跑一遍看着像」：

1. 加一个临时探针命令 `xtask test enumdump`，把**四路枚举的每条记录全字段**（名字、源路径、
   产物路径、entry、expected、interpOnly、kind、bucket…）逐行 dump；
2. 在**改动前**的树上跑一次存基线（1521 行：ASSETS 297 / CORPUS 614 /
   DIST-interp 288 / DIST-jit 284 / VM-interp 286 / VM-jit 282）；
3. 重构后再跑一次 —— **`diff` 输出 0 行**：不只集合与条数一致，**逐条顺序也逐字节一致**；
4. 删除探针（不进仓库），`xtask test` 全绿。

## 状态

🟢 完成
