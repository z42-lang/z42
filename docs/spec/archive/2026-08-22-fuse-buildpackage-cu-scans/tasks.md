# Tasks: fuse-buildpackage-cu-scans（流程优化 F4+F12）

> 状态：🟢 已完成 | 创建：2026-08-22 | 完成：2026-08-22 | 类型：refactor（纯内部，字节不动点）

**变更说明：** `IrDump.BuildPackageCus` 在符号收集（`CollectAll` + `VarFieldInfer`）之后，对同一批
`cus[]` 做 **4 趟彼此无数据依赖的全 CU 扫**——`_pkgClassNs`（classNs：跨-ns 类名解析图）/
`_pkgLocalClasses`（localClasses：G18 本地类集）/ `pkgClassMap`（跨文件基类 map）/ `allFuncs`（全包自由
函数聚合）。F4+F12 把这 4 趟**融成一趟**外层 `while (i < count)` 遍历，一次 decl-walk 同时建 3 张 map
（classNs / localClasses / pkgClassMap）+ 在同一外层调用 `ExtractFuncs` 聚合 allFuncs。

**为什么字节不动点天然成立：** 4 者互不依赖、且都按 `cus[]` 同序、`Decls[]` 同序遍历，融合后每张 map 的
**写入顺序与命中/首赢（`!ContainsKey`）语义逐条不变** → 最终 map 内容与融合前逐一相等。classNs 预置
`imported.ClassNamespaces`（照旧）后再按 CU 收本地类；allFuncs 追加序 = ExtractFuncs 逐 CU 序（不变）。
`_injectGlobalUsings` / `CollectAll` / `VarFieldInfer` / `_buildMergedPartial` 有真实数据依赖，保持在前不动。
单文件 emit 路径（`_compileOne`）仍调 `_pkgClassNs` / `_pkgLocalClasses` 辅助（1-CU 场景）→ 辅助函数保留。

**实测收益（A/B swap，只换 `z42c.semantics.zpkg`，其余 driver/VM/libs 全同，interp 全量重编 workspace
24 库，clean/mod 交替 3 轮）：** **再次证伪先验「high」**——clean 均值 **33.56s**（min 33.41）vs mod
均值 **33.75s**（min 33.39），**delta +0.18s（+0.5%），在 ±1s 跨轮噪声内、min 值几乎相等（33.41 vs
33.39）→ 墙钟不可测（~0%）**。原因同 F3：融合的 4 趟是纯 StrMap-building AST 扫，相对每文件
typecheck/codegen/DepScan/zbc-write 主导的编译总时间占比极小。**本项定性为字节透明的冗余清理 refactor
（4 趟全 CU 扫 → 1 趟，去掉 3 趟重复 decl-walk + 每包小幅降遍历），非 perf 杠杆。**

> **程序级结论（诚实修正）：** F 程序里唯一**实测**到墙钟收益的杠杆是 **F2（DepScan 进程级 memo，
> 消 O(N²)，-71%）**。F1（~2%）/F3（~0%）/F4+F12（~0%）这类「prelim-scan 融合 / 单趟抽取」对 33.5s
> 全量编译均不可测——真杠杆只在 **O(N²) 跨成员重复**（F2）或**结构性解锁**（F8 包级 IrModule）处。
> F5/F6/F7（_optFunc / 逃逸分析 / LICM 共享 memo）同属 prelim-scan 类，预期同为 ~0% 墙钟、仅清理价值。

**文档影响：** 无。纯内部字节不动点重构；融合的 4 趟均属既有机制，改动的「为什么可安全融成一趟」以
`IrDump.BuildPackageCus` 源码头注承载，不新增 book 机制页。README 六段不涉（无文件增删 / 无对外入口变化）。

## 任务
- [x] 1.1 `IrDump.BuildPackageCus`：把 classNs / localClasses / pkgClassMap / allFuncs 4 趟全 CU 扫融成
      一趟外层循环（classNs 预置 imported、单趟 decl-walk 建 3 map + 同层 ExtractFuncs 聚合）
- [x] 1.2 保留 `_pkgClassNs` / `_pkgLocalClasses` 辅助（单文件 emit 路径仍用）
- [x] 1.3 字节不动点守卫：modified vs clean 编出 **24/24 stdlib zpkg sha256 逐字节一致 ✅**
- [x] 1.4 性能对比（A/B swap，仅换 semantics zpkg）3 轮：clean 33.56s vs mod 33.75s，**噪声内 ~0%**

## 验证
- [x] V1 完整 `xtask test` 全绿（C#-free）：**self-host 不动点 5/5 gen1==gen2** · z42c [Test] · e2e ·
      cross-zpkg · stdlib · vscode-syntax → `✅ GREEN — all stages passed (C#-free)`（exit 0）
- [x] V2 base=origin/main tip `43379dcb`（F3 #250 后），最新 nightly SDK-41 供种本地编译，无需 CI 兜底
