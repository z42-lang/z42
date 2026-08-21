# Tasks: perf-single-export-extract（流程优化 F3）

> 状态：🟢 已完成 | 创建：2026-08-22 | 完成：2026-08-22 | 类型：refactor（纯内部，字节不动点）

**变更说明：** `IrDump.BuildPackageCus` 的**两趟导出抽取**里各去掉一次冗余全量 `ExportedTypeExtractor.Extract`：

- **pass-1（全包自由函数聚合）**：此前对每文件跑**全量** `Extract(cus[i], symbols, "")`——materialize 全部
  class/interface（含 11 内建）/enum/delegate（含 11 内建）/impl——却**只留 `own.Functions`**、其余全丢弃。
  改为新增 `ExportedTypeExtractor.ExtractFuncs(cu, symbols)` 只遍历 `MethodDecl` 抽自由函数。
- **pass-2（每文件 TSIG）**：`ExtractP` 内部 `Extract` 抽出的每文件自由函数**必被全包 `allFuncs` 覆盖**，
  故给核心加 `skipFuncs` 开关（`_extractCore(..., skipFuncs)`），`ExtractP` 走 `skipFuncs=true` 跳过该次
  函数抽取。class/interface/enum/delegate/impl 抽取一字不改。

**为什么字节不动点天然成立：** `_extractFunc` 只依赖 `md`+`symbols`，与 `ns`/`pkgClassMap`/`classNs` 无关
→ pass-1 func-only 与旧全量 Extract 的 `Functions` 列表**逐条同序等价**；pass-2 跳过的函数本就被 `allFuncs`
覆盖、从不进输出。故导出面/TSIG/zbc 全部不变。

**实测收益（诚实修正先验）：** 计划先验估「high」，A/B 实测**证伪**——只换 `z42c.semantics.zpkg`
（其余 driver/VM/libs/zpkg 全同）跑 `build --workspace --release`（interp，全量重编）3 轮交替：
modified 均值 **33.27s** vs clean **33.04s**，**差在 ±0.7s 噪声内（~0%）**。抽取仅占 interp 全量编译
（typecheck/codegen/DepScan/zbc-write 主导）极小份额 → 墙钟不可测。**本项定性为字节透明的冗余清理
refactor（去除真实双重抽取 + 每文件小幅降分配），非 perf 杠杆**；真正的大杠杆转 F4+F12（CU 多趟扫融合）。

**文档影响：** 无。纯内部字节不动点重构，无外部可见行为/机制/规则变更；两趟抽取属既有机制，改动的
「为什么 func-only / 为什么可 skipFuncs」以源码头注承载（ExportedTypeExtractor.z42 `ExtractFuncs`/
`_extractCore`/`ExtractP`），不新增 book 机制页。

## 任务
- [x] 1.1 新增 `ExportedTypeExtractor.ExtractFuncs(cu, symbols)`：func-only 抽取（返回精确大小数组）
- [x] 1.2 `Extract(5参)` 委托 `_extractCore(..., skipFuncs=false)`；`_extractCore` 的 `MethodDecl` 分支
      加 `!skipFuncs` 守卫
- [x] 1.3 `ExtractP(6参)` 改走 `_extractCore(..., skipFuncs=true)`（函数将被 allFuncs 覆盖）
- [x] 1.4 `IrDump.BuildPackageCus` pass-1 循环改用 `ExtractFuncs`（不再全量 Extract 再丢弃）
- [x] 1.5 字节不动点守卫：modified vs clean 编出 **24/24 stdlib zpkg sha256 逐字节一致 ✅**
      （仅 `z42c.semantics.zpkg` 因含本次新代码而异，符合预期）
- [x] 1.6 性能对比（A/B swap，仅换 semantics zpkg）：modified 33.27s vs clean 33.04s，**噪声内 ~0%**
      （诚实结论：非 perf 杠杆）

## 验证
- [x] V1 完整 `xtask test all` 全绿（C#-free）：**self-host 不动点 5/5 gen1==gen2 逐字节** · z42c [Test]
      26 units · e2e 2/2 · vscode-syntax → `✅ GREEN — all stages passed (C#-free)`（TEST_EXIT=0）
- [x] V2 base=origin/main tip `8e38b910`（PR4c #248 后，最新 nightly SDK-41 供种可本地编译，
      无 memory 里旧 tip 9336f322 的 DiagnosticCodes 引导窗口问题）→ 本地全 GREEN，无需 CI 兜底判定。
