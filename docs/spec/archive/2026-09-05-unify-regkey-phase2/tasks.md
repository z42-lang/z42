# Tasks: 移除方法注册键的读侧回落（unify-regkey-phase2）

> 状态：🟢 完成（PR 待合并）| 创建：2026-09-05 | 见 [proposal.md](proposal.md)
> 设计沿用 [`archive/2026-09-04-unify-regkey-source-of-truth/design.md`](../../archive/2026-09-04-unify-regkey-source-of-truth/design.md)
> §2（6 个消费点现状表）与 §3（收敛后形态），本文件只记阶段 2 的增量。

## ① User 在 #427 上要求的回归确认：`dict_set_get` +14.5%

- [x] 1.1 认清陷阱：#427 已进 main → bench 门比的是「PR vs merge-base」，回归（若真）
      已成新基线，**门不会再报**，只能显式 A/B。
- [x] 1.2 **决定性证据 = 字节对账，而非计时**（计时在同字节码上只能测噪声）：
      | 对象 | 做法 | 结果 |
      |---|---|---|
      | bench 自身 | 同 cwd、同 `Z42_LIBS`，老/新 z42c 各 `--emit-zbc src/libraries/z42.core/bench/core_bench.z42` | **逐字节相同**（8806 B）|
      | `z42.core.zpkg` | 源码拷到固定路径 `/tmp/ab/core-src`（消除嵌入的绝对路径差），老/新 z42c 各 `build --release` | **逐字节相同** |
      | VM | `git diff 66ec776f d785a322 -- src/runtime` | **空**（#427 只动 z42c 源）|
- [x] 1.3 结论：**同一份字节码 + 同一个 VM** → `ratio=1.1449` 与 #427 无因果关系，是该
      micro-bench 的 runner 噪声。**证伪，无需修复。**
- [x] 1.4 记下方法：此前「本地建不起 bench」的缺口在于试图走 bench harness；实际
      `z42vm <driver> -- --emit-zbc <bench.z42> <out>` 就是 harness 编 bench 单元的原样命令
      （`_compilePrep`，`scripts/test/xtask_test_lib_units.z42:275`），单文件直编即可。
- [x] 1.5 跨 worktree 比字节**不成立**：`z42.core.zpkg` 的 STRS 段嵌了源文件绝对路径，
      两个 worktree 路径不同 → 必然差几百字节。**必须同 cwd 比。**

## ② 阶段 2：移除读侧回落

### 前置（阶段 1 tasks 列的硬前置）

- [x] 2.1 补**跨-CU partial 的 TSIG 导出**测试：`src/tests/cross-zpkg/partial_crosscu_export/`
      —— decl 碎片（`target/src/CalcDecl.z42`）与 impl 碎片（`CalcImpl.z42`）分处不同 CU，
      经 `ext`（一层跨包）与 `main`（两层）双路径消费；含实例与 static 两种 partial。
- [x] 2.2 补 `static partial` decl-only 用例：`src/tests/partial-types/partial_static_method.z42`
      （含「无实现 → 整体擦除」的 `OnUnusedStatic`）。静态键走**全签名 mangle**，
      与实例的 primary/非-primary 规则不同，此前全仓库无此写法。
- [x] 2.3 对账 `_checkExposure` 的 E0441 条数：无 golden 依赖条数
      （`access_control_tests.z42` 全部走 `SemanticDump.FirstErrorCode`，只断言首条码）；
      且回落只在 `RegKey == ""` 时触发，2.4 的探针实测零触发 → 条数不可能变。

### 实施

- [x] 2.4 `OverloadResolver.MethodKeyOf` 收窄：删回落 → `RegKey == ""` 即 `throw`
      （**这个 throw 同时就是探针**：还依赖回落的路径会在编译期炸）。
- [x] 2.5 去掉不再需要的 `owner` 形参；6 个调用点同步（`ClassExtractor` ×2 /
      `DeclBinder` ×3 / `IrGenTypeEmitter` / `IrGenMemberEmitter`）。
- [x] 2.6 注释同步：`Decl.z42` 的 `RegKey` 字段（读法 + 不变量）、`MemberCollector`
      擦除分支（**它现在是这类方法唯一的键来源**，并写明两侧键为何一致 / 何时不成立）、
      `SymbolCollector.RegisterMethod`（读侧不再兜底 → 新注册路径必须走本入口）。

### 验证

- [x] V1 字节中性：阶段 2 的 z42c 与 main 的 z42c 各编一遍未改动的 `z42.core`（同 cwd）→ **逐字节相同**
- [x] V2 `xtask build compiler`（自建，guard 在编 z42c 自身源码时零触发）
- [x] V3 `xtask test` 全量 **✅ GREEN**（0 失败），含自举不动点 **3/3 gen1==gen2**
- [x] V4 `xtask test stdlib --mode jit` 两 shard：1/2 → 169 文件全过；2/2 → 150 文件全过
- [x] V5 `xtask test bootstrap`：nightly z42c 编得动当前源 → 无越界
- [x] V6 归档 + 更新阶段 1 归档里的「阶段 2 另 PR」指针

> ⚠️ 踩过一次**自造的假红**：在一轮 `xtask test` **跑到一半时改了源**（订正一行 `using`），
> 而不动点比的是「跑开始时快照的 gen1」vs「此刻用新源重建的 gen2」→ `z42c.semantics`
> 失配（两代**大小相同**、只差内容派生字段），另两个包逐字节相同 —— 失配包恰是被改的那个。
> **GREEN 期间不要动源码**；干净重跑即 3/3。

## 明确不做

- 不改键格式/规则 → 无格式 bump、无两代自举
- 不并入 impl 方法到 primary/非-primary
- 不动 `CallEmitter` 的静态 DepIndex 查找
