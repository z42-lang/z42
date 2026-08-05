# Proposal: 循环内分配 hoist + 对象复用（loop allocation hoisting & reuse）

## Why

`add-escape-analysis-stack-alloc`（PR #115）落地后量测发现：**循环体内每迭代 `new` 临时对象/数组**是
栈分配最想优化、却收益最弱的场景——per-call 非逃逸对象 1.15×，但**循环体内**只有 1.01×（打平）。根因：
v1 帧级 arena 只在函数帧退出时释放，热循环里每迭代的栈对象**累积到函数返回**（8M 迭代 = 1600 万条目）。

更根本的解（User 提出）：**把分配提到循环外、每迭代复用同一块存储（只重初始化）** → **只分配 1 次**，
消除累积、消除 per-迭代分配开销。堆分配（逃逸分析证不出但迭代内可复用者）受益最大（N 次 malloc+GC → 1 次），
栈分配也受益（N 次 arena push + 累积 → 1 次 + 常量内存）。这是业界 *allocation hoisting / object reuse*
（HotSpot escape-analysis + scalar-replacement 类似）。

不做的话：循环内分配这个最常见的热点模式，收益停在 1.01×，且 arena 内存随循环规模线性膨胀。

## What Changes

- **新编译器 pass `IrLoopAllocReuse`**（复用 `IrLicm` 的 CFG/支配/自然循环/干净 pre-header 机件）：
  识别自然循环体内**迭代内可复用**的 `ObjNew` / `ArrayNew`，把分配 hoist 到 pre-header（只分配一次），
  循环体内改为**重初始化**复用的存储。
- **对象机制（无格式变更）**：pre-header emit `ObjNew(class, ctor="", [])`（空 ctor 名哨兵 = 裸分配，
  走运行时 `outcome=None` 路径）；循环体原址 emit `Call ctor(%obj, args)` 重初始化（运行时本就这样调 ctor）。
- **数组机制**：`ArrayNew(size, elem)` 的 size 循环不变 → 直接 hoist 到 pre-header；循环体依赖既有元素
  写回重初始化（要求「读前必写全」）。
- **开关**：`Opt.LoopAllocReuse`（新 bit）；release 开 / debug 关；CLI `--no-opt loop-alloc-reuse`；toml `[optimize]`。
- **诊断**：编译期开关 `--no-opt loop-alloc-reuse` 关本 pass → 开/关**输出逐字节对拍**（主正确性门）；
  IR dump 隔离可见。（初稿的运行时旁路/断言已删——本 pass 是纯编译期变换，见 design D6 修正。）
- **运行时**：**无需改动**——空 ctor 名 `ObjNew` 的 `outcome=None` 裸分配路径已天然优雅（实施首验确认，
  `func_index.get("")`/`try_lookup("")` 皆 None → 跳过 ctor → `frame.set(dst)`，不报错）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/IrLoopAllocReuse.z42` | NEW | 新 pass：识别 + hoist + reinit 变换 |
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | 加 `LoopAllocReuse` bit + `All` + `ByName` |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | pipeline 挂入（stack-alloc 之后，需逃逸结果） |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | `Opt.All - ...` 两处补新 bit（IR dump 隔离） |
| `src/compiler/z42c.semantics/src/IrLoopUtil.z42` | NEW | 从 IrLicm 抽出的共享 CFG/循环机件（refactor） |
| `src/compiler/z42c.semantics/src/IrLicm.z42` | MODIFY | 委托 IrLoopUtil（refactor，行为不变） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件登记新 pass + IrLoopUtil |
| `src/compiler/z42c.semantics/tests/loop-alloc-reuse/` | NEW | pass 单测（hoist 命中 / 不命中边界） |
| `src/tests/run/loop-alloc-reuse-*/` | NEW | e2e golden（对象 + 数组复用，结果与不开时一致） |
| `docs/book/src/compiler/optimization.md` | MODIFY | 新 pass 机制 / 条件 / 变换（知识上浮） |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index：登记本 change 解决 escape-stack-future ④ 一部分 |

**只读引用：**
- `src/compiler/z42c.semantics/src/IrLicm.z42` — 复用 CFG/支配/自然循环/pre-header 机件
- `src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42` — 复用 `StackAlloc` 结果 + `_ctorLeaksThis` this-safe 判定
- `src/libraries/z42.ir/src/IrInstr.z42` — ObjNew/ArrayNew 结构（不改）
- `src/runtime/src/interp/exec_object.rs` obj_new — ctor 调用约定参考

## Out of Scope

- **ArrayNewLit（字面量数组 `[a,b,c]`）**：其元素在创建时初始化，复用需把元素写移到循环体（额外手术）→ v1 不做，design Deferred。
- **JIT 侧**：本 pass 是编译期 IR 变换，JIT/interp 都受益（变换后的 IR 对两者一致）；无需 JIT 专门改动。
- **跨过程可复用性**：v1 只做单函数内、单自然循环体内的分配；跨过程摘要另立 change。
- **scope/回边 arena 复位**（deferred escape-stack-future ④）：与本 change 互补但正交，另立。

## Open Questions

- [ ] 运行时空 ctor 名 `ObjNew` 的 `outcome=None` 路径当前是否已优雅跳过（不报「ctor not found」）？→ 实施首验。
- [ ] 数组「读前必写全」的 v1 判据收多紧（仅常量下标覆盖 [0,size)，还是含简单循环写）？→ design D4 定。
