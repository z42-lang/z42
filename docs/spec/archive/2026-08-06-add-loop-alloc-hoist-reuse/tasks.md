# Tasks: 循环内分配 hoist + 对象复用

> 状态：🟢 已完成 | 创建：2026-08-05 | 完成：2026-08-06

## 进度概览
- [x] 阶段 1: 前置验证（运行时裸分配路径已天然优雅 + IrLoopUtil 抽取）
- [x] 阶段 2: 核心 pass 实现（识别 + 变换）
- [x] 阶段 3: 开关 / 诊断 / pipeline 挂入
- [x] 阶段 4: 测试与验证（GREEN 全绿 + 开/关对拍 + 量测）

## 阶段 1: 前置验证
- [x] 1.1 验运行时 `obj_new`（exec_object.rs）：`ctor_name==""` → `outcome=None` 裸分配**已天然优雅**
      （`func_index.get("")`/`try_lookup("")` 皆 None → 跳过 ctor → `frame.set(dst)`，不报错）→ **运行时无需改动**
- [x] 1.2 抽 `IrLoopUtil.z42`（LoopCfg + BuildCfg/Headers/LoopBody/CleanPreheader/BlockIdx）供 LICM + 本 pass 共用；
      `IrLicm` 委托之（refactor，行为不变，已构建验证 z42c 自建通过）

## 阶段 2: 核心实现（IrLoopAllocReuse.z42）
- [x] 2.1 骨架 `Run(IrModule m)` → 每函数 BuildCfg + Headers + LoopBody + CleanPreheader（复用 1.2）
- [x] 2.2 候选扫描：循环体内 `ObjNew` / `ArrayNew`，`StackAlloc==true`
- [x] 2.3 C2 迭代内局部：前向 copy 闭包不含多赋值 reg（`_iterationLocal`）判「不跨迭代携带」
- [x] 2.4 C3 形状固定：ArrayNew.Size 不在循环体内定义（`defInLoop` 判）
- [x] 2.5 C4 重初始化完整：对象走 ctor 单基本块（StackAlloc 已含 this-safe）；数组走「常量下标读前写全」单块线性(D4)
- [x] 2.6 变换-对象：pre-header 追加裸 ObjNew(ctor="") + 循环体原址 → `Call ctor(%r, args)`（dummy dst）
- [x] 2.7 变换-数组：ArrayNew 移 pre-header

## 阶段 3: 开关 / 诊断 / 挂入
- [x] 3.1 OptSet.z42：`LoopAllocReuse=128` bit + `All=255` + `ByName("loop-alloc-reuse")`
- [x] 3.2 IrOptPipeline.z42：`IrEscapeAnalysis.Run` 之后挂 `IrLoopAllocReuse.Run`（Opt.Has 门控）
- [x] 3.3 IrDump.z42：两处 `Opt.All - Opt.Inline - Opt.StackAlloc` → 补 `- Opt.LoopAllocReuse`
- [x] 3.4 诊断：编译期开关旁路（`--no-opt` 开/关对拍）为主门；运行时旁路/断言**已删**（纯编译期变换，design D6 修正）

## 阶段 4: 验证
- [x] 4.1 cargo build (z42vm) 无错（z42c 自建通过）
- [x] 4.2 e2e golden（`src/tests/optimization/loop_alloc_reuse{,_carried}/`）：命中（对象+数组，输出 50）+
      C2 安全兜底（循环携带链表不复用，输出 6）——4/4 interp+jit（4.3 合并；单测以 e2e 场景覆盖，符合"≥1 正常+1 边界"）
- [x] 4.3 e2e golden 开/关一致，interp + JIT 双跑（并入 4.2）
- [x] 4.4 `xtask test compiler` z42c 自举 5/5 gen1==gen2 byte-identical 全绿（本 pass 作用于编译器自身）
- [x] 4.5 `xtask test` 完整 GREEN gate — ✅ all stages passed (C#-free)
- [x] 4.6 量测：循环体基准（`new Point`+`new int[3]`×8M）reuse ON vs OFF（两者 escape-stack 皆开）：
      **interp 2.91× / jit 4.09×**（System 0.002s vs 0.22–0.30s）；开/关 zpkg 字节 differ（pass 确实生效）+ 输出一致
- [x] 4.7 文档同步：semantics README 六段（新 pass + IrLoopUtil）+ book optimization-pipeline（pass 2e 全文 + OptSet 位串）
- [x] 4.8 spec scenarios 逐条覆盖确认（命中 obj/arr + 4 不命中边界 + 开关 + escape 诊断）

## 备注
- ArrayNewLit 元素初始化复用需元素写手术 → v1 Out of Scope，记 design Deferred。
- 与 scope-reset（escape-stack-future ④）互补，不冲突（D5）。
- **实测校正**：design D5 初稿预测"增量收益偏小"被推翻——实测 interp 2.91× / jit 4.09×（arena 累积代价被低估）。
- roadmap Deferred Index：本 change 解决 escape-stack-future ④ 的循环内累积（部分）；ArrayNewLit / 全外提嵌套循环 留后。
