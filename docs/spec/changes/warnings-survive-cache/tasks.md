# tasks: warnings-survive-cache

> 类型：**fix**（警告的可见性取决于与缺陷无关的状态）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「批 B：给静默的地方装门」——两个警告静默器那条。

## Why

### ① 警告只在冷构建那一次可见（实测）

带 W0700（switch 对 enum 不穷尽）的工程连续构建三次，**源码一字未改、缺陷还在**：

| 构建 | 缓存状态 | W0700 |
|---|---|---|
| ① 冷 | `cached: 0/1` | ✅ 打印 |
| ② 什么都不改 | `cached: 1/1` | ❌ 消失 |
| ③ 再来一次 | `cached: 1/1` | ❌ 消失 |

「连着构建两次」恰恰是最常见的工作方式 ⇒ 实际效果是**警告基本看不见**。

两个**独立**的静默器叠在一起，各管一种缓存形态：

| 形态 | 静默器 |
|---|---|
| 整包全命中 | driver 在 `no changes; preserved` 处**早退**，压根不编译 ⇒ 无人呈现警告 |
| 部分命中 | `CompileCuTask.Run` 的 cached 分支把 `DiagMsgs` 置空（`IrDump.z42`），而无人回填 |

只修一格没有意义：修了部分命中那格，最常见的「构建两次」照样静默（我第一版就是这样，
探针当场打脸）。

### ② generator 产出的 CU 里的警告一条也不打印

`Main.z42` 的警告循环上界是 `srcs.Length`，而 `cms` 是 **union**（用户 CU + 生成 CU），
比 `srcs` 长。紧挨着上面的**错误**循环在 `fix-generated-cu-errors-invisible` 里已经改成
`art.CmCount` 并写明了理由，这一条当时没跟着改。

少一格不像错误那格会崩（越界读 `srcs[ei]`），所以它更安静、也更容易一直留着 ——
这正是「同一个 bug 的两半只修了一半」的标准形态（本仓已有多例）。

## What Changes

| 处 | 改动 |
|---|---|
| `CacheStore.z42` | `MetaVersion` 6 → **7**；新增 `diag <hex>` 行（读 + 写）；`CacheMeta.Diags/DiagCount` |
| `PackageCompile.z42` | `CachedNsMeta` 加 `Diags/DiagCount` 并在既有回填循环里回填；新增 `CuDiagSnapshot` + `CompileArtifacts.CuDiags` 快照 |
| `IncrementalDriver.z42` | `WriteMetas` 收 `cuDiags`，写 `diag` 行 |
| `BuildCache.z42` / `Main.z42` | 透传 `art.CuDiags` |
| `Main.z42` | preserved 早退前回放警告；警告循环上界 `srcs.Length` → `art.CmCount` + `<generated>` |

### 🔴 为什么快照点不能取 `cms[i].DiagMsgs`

`EnforceFileScopeAll`（E0436）与 `_runAnalyzers` 都在其后追加，且**每次构建都会重新发**
（analyzer 跑在 AST 上，cached CU 的 AST 是在的）⇒ 一并存进 meta 就会在命中时**重复打印**。
快照必须在这两层之前取，所以要专门开一条 `art.CuDiags` 通道。

顺带：`ErrorCount` 不回填。`ErrorCount > 0` 的编译**根本不写 cache** ⇒ 存下来的只会是
warning。这个前提写进了注释 —— 它哪天变了，回填处必须跟着改，否则命中会把错误降级成警告
（比丢警告坏得多）。

### 为什么用包装类而不是 `string[][]`

**z42 不原生支持锯齿数组**（`z42.cli/ArgParser.z42:55` 里 `_MutexGroup` 是同一个办法、
同一个理由）。我差点直接写 `string[][]`。

### 为什么 bump `MetaVersion` 而不是 `CompilerFingerprint`

`MetaVersion` 6→7 本身就整批作废旧条目，是这一档正确的旋钮：产物字节不变、编译器语义
没变，变的只是 meta 多一种行。`CompilerFingerprint` 不动。

## Scope（允许改动的文件）

- `src/compiler/z42c.pipeline/src/CacheStore.z42`
- `src/compiler/z42c.pipeline/src/PackageCompile.z42`
- `src/compiler/z42c.driver/src/{Main,BuildCache,IncrementalDriver}.z42`
- `scripts/test/xtask_test_incremental.z42`（新门 `_warningsSurviveCache`）
- `docs/internals/src/compiler/project-model.md`

## Tasks

- [x] `diag` 行入 meta（v7）+ `CachedNsMeta` 回填（部分命中那格）
- [x] preserved 早退前回放警告（全命中那格）
- [x] 警告循环上界 → `art.CmCount` + `<generated>`
- [x] 实测三臂：冷 / 全命中 / 部分命中，警告都在
- [x] 新门 `_warningsSurviveCache`（判据看 **stderr**，不是产物字节）
- [x] 阴性对照：逐格撤回，对应那一臂**恰好**变红（A 撤 preserved 回放 ⇒ 只 ② 红；B 撤 cached 回填 ⇒ 只 ③ 红）
- [x] `xtask test incremental` 全绿
- [x] `xtask test compiler` 全绿（24 组 0 failed）+ 自举不动点 3/3 gen1==gen2
- [ ] GREEN：CI 全矩阵绿

## 判据为什么必须看 stderr

本门其余各轮全在比**产物字节**，而这个缺陷**不动产物一个字节** —— 增量与全量编出来的 zbc
一直相同，丢的只有终端上那几行。「增量对账全绿」与「警告一条都看不见」可以同时成立，
所以旧门对它是结构性地瞎的，不是恰好没覆盖。

## 不做（Out of Scope）

- **不扩 `depsId`**。与该键历史上那四次（`[build] incremental` / `[optimize]` / `[syntax]`）
  正好相反：源码真的没变，强行失效等于用一次全量重编换几行输出。缺的不是**失效**是**呈现**。
  判断形状的问法：「重编一遍能得到新答案吗」——不能，就是呈现问题。
- **generator 那格没有专门的夹具测试**。它是与错误循环逐字对称的两行改动，错误那格已有测试；
  为它造 generator 工程夹具是另一条线的量。如实记在这里，别当成已覆盖。
- 不碰 W0700 之外的警告码，也不改任何诊断的严重级。
