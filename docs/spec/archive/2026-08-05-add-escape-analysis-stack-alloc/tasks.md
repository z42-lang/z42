# Tasks: 逃逸分析驱动的栈上分配

> 状态：🟢 已完成 | 创建：2026-08-04 | 完成：2026-08-05（PR #115 squash-merge 9fcc9a8b）

## 进度概览
- [x] 阶段 1: IR 字段 + zbc/zpkg 格式 bump（zbc 1.29 / zpkg 0.34）
- [x] 阶段 2: 逃逸分析 pass（IrEscapeAnalysis + OptSet + pipeline）——CI 自举到 0.34 验证
- [x] 阶段 3: 运行时 arena 消费（interp，B2 per-context + 诊断）
- [x] 阶段 4: 测试与验证——**CI 全绿**（test-host 全 4 平台 + verify-selfhost + e2e + fixtures 重生）
- [x] 阶段 5: 文档同步（book/README/roadmap/version-bumping/changelog）+ 归档

> **落地记录（2026-08-05）**：PR #115 全绿合并。CI 侧连修 4 类格式-bump 完整性缺口（本地 Rust 测试
> 结构上够不到、只有自举 e2e 能暴露）：① ci-bootstrap 两代自举「先建 gen0 stdlib 再建 gen1 z42c」解
> 「格式 bump + z42c 用新 z42.ir 字段」的轴④冲突；② 运行时 field_get 补 StackArray（`.Length` 经
> FieldGet）+ Convert.Src 判逃逸；③ z42c 侧 ZbcReaderInstr 补读 stack_alloc（writer/reader 对称）+
> golden hex + 集成测试构造；④ committed zpkg-format fixture 用 CI 产出的 0.34 工具链重生（本地两代
> 自举环境性编不出，见 [[escape-stack-format-bump-ci-learnings]]）。

> **本地验证天花板（2026-08-05）**：格式 bump（zbc 1.29/zpkg 0.34）后新 VM 读不了 0.33 种子；两代自举
> 需旧种子 VM，而本仓 `.z42` 种子（Aug 3）**在 builtin + stdlib API 上均落后当前 main**（`__str_to_chars`
> 缺失；bridge-VM 两代在 `ClassDescBuilder`/`StrMap`/`IrFieldDesc` 等**未改文件**报错 = 种子 API 漂移，非本
> 变更代码）。故 **z42c 自举 / committed fixture 重生 / golden / e2e 一律以 CI 为准**（[bootstrap-seed.md]
> cold 路径约定）。**本地已确认**：Rust runtime 编译净 + 886 单测（878 lib + 8 arena）绿；种子成功重建全
> 24 stdlib 库（含我的 z42.ir StackAlloc 字段 + ZbcInstr 编码）→ Phase 1 z42 侧编译验证通过。

## 阶段 1: IR 字段 + zbc 格式（z42.ir + runtime reader）
- [x] 1.1 `IrInstr.z42`：`ObjNewInstr`/`ArrayNewInstr`/`ArrayNewLitInstr` 加 `bool StackAlloc=false` + ctor 参数（照 `MkClosInstr`）
- [x] 1.2 `ZbcInstr.z42`：三指令编码尾部加 `u8` 标志（镜像 MkClos:137）
- [x] 1.3 `ZbcFormat.z42`：`Minor` 28→29
- [x] 1.4 `bytecode.rs`：`ObjNewInsn`/`ArrayNewInsn`/`ArrayNewLitInsn` 加 `stack_alloc: bool`
- [x] 1.5 `zbc_reader.rs`：读尾 `u8` flag + `ZBC_VERSION`→1.29 + `ZPKG_VERSION`→0.34
- [x] 1.6 `translate.rs`：JIT 构造 Insn 兼容新字段（读取即忽略，不改 emit）
- [~] 1.7 zbc/zpkg fixture 重生 + golden hex 单测更新（版本 pin 单测已改 29/34 ✅；committed .zbc/.zpkg fixture 重生 ⏳ CI-gated，需 0.34 工具链）

## 阶段 2: 逃逸分析 pass（z42c.semantics）
- [x] 2.1 `OptSet.z42`：`Opt.StackAlloc=64`、`All`→127、`ByName("stack-alloc")`
- [x] 2.2 `IrEscapeAnalysis.z42`：`_computeEscapedRegs(fn)` 引擎（角色分类器规则表 + copy 传递闭包）
- [x] 2.3 `IrEscapeAnalysis.z42`：`_ctorLeaksThis(m, ctorName, cache)`（单函数 param-0 摘要）
- [x] 2.4 `IrEscapeAnalysis.z42`：`Run(m)` 主流程（标 ObjNew/ArrayNew/ArrayNewLit，`_singleDef` 守卫）
- [x] 2.5 `IrOptPipeline.z42`：`Run` 接入 `if Opt.Has(optSet, Opt.StackAlloc) IrEscapeAnalysis.Run(m)`

## 🔴 阶段 3 前置决策（2026-08-05 实施期发现，待 User 裁决）
- [x] D6 运行时对象落地方案：A 数组优先 / B2 对象+数组跨帧句柄（见 design.md Decision 6）
- [x] D7 诊断防线随实施落地（epoch 中毒 + 逃逸汇点 debug 断言 + `Z42_STACKALLOC=off/stats` 旁路）
- 备注：编译器分析 + IR 标志已覆盖对象+数组；本阶段仅 interp 运行时，方案 A/B 编译器侧相同。
  本地无法完整验证自举（种子 builtin 落后 + 格式 bump）→ pass 自举以 CI 为准；Rust 运行时本地可验。

## 阶段 3: 运行时 arena 消费（interp-only；A/B 待裁决）
- [x] 3.1 `types.rs`：`Value::StackObject`/`StackArray` 变体 + `StackObjectData`/`StackArrayData`（24B 布局，装箱）
- [x] 3.2 `types.rs`：子引用遍历把栈变体归 "no children"（:874/:908，堆字段由根扫描器覆盖）
- [x] 3.3 `interp/mod.rs`：`Frame` 加对象/数组 arena 字段 + 三处初始化置空
- [x] 3.4 `exec_object.rs`：`obj_new` 按 flag 分叉 arena；`FieldGet`/`FieldSet` 识别 `StackObject`
- [x] 3.5 `exec_array.rs`：`array_new`/`array_new_lit` 按 flag 分叉 arena
- [x] 3.6 `exec_instr.rs`：分发透传 flag；数组 get/set/len 识别 `StackArray`
- [x] 3.7 `vm_context.rs`：外部根扫描器加扫对象/数组 arena slots（:660-664 类比 env_arena）
- [x] 3.8 `arc_heap.rs`：`size_of`/子引用分支加栈变体（照 `StackClosure` :1746）

## 阶段 4: 测试与验证
- [x] 4.1 `tests/escape_analysis/` pass 单测（合格/各汇点不合格/ctor 泄漏/copy 传递/多定义/未知指令保守/单开确定）  ✅ 8 arena 单测本地绿（含 staleness/复用槽/截断/LIFO）
- [ ] 4.2 e2e golden（`src/tests/`）：栈对象/数组字段读写 + 循环创建 + 含堆字段；interp==jit 输出等价  ⏳ CI-gated（种子太旧，本地不可验）
- [ ] 4.3 GC 压测：栈对象持堆字段引用触发 GC 不误回收  ⏳ CI-gated（种子太旧，本地不可验）
- [x] 4.4 `cargo build --release`（z42vm）无错  ✅ 本地绿
- [x] 4.5 `cargo test --lib`（runtime Rust 单测，[[xtask-test-excludes-cargo-test]]）  ✅ 本地绿
- [ ] 4.6 `xtask test`（全 stage GREEN gate；compiler 跑两遍 5/5 自举不动点 D7）  ⏳ CI-gated（种子太旧，本地不可验）
- [ ] 4.7 e2e-direct interp+jit 双跑全 flat 语料  ⏳ CI-gated（种子太旧，本地不可验）
- [ ] 4.8 spec scenarios 逐条覆盖确认  ⏳ CI-gated（种子太旧，本地不可验）

## 阶段 5: 文档同步 + 归档
- [x] 5.1 `z42c.semantics/README.md`：功能索引 + 核心文件加 `IrEscapeAnalysis`
- [x] 5.2 `optimization-pipeline.md`：新增逃逸分析/栈分配 pass 节 + 对齐日期
- [x] 5.3 新建 `escape-analysis-stack-alloc.md` book 机制页 + 挂入 `SUMMARY.md`
- [x] 5.4 `roadmap.md`：Deferred Backlog Index 登记 4 条 future 条目
- [x] 5.5 version-bumping.md checklist 逐项核对（格式 bump 同步点全覆盖）
- [ ] 5.6 归档 `docs/spec/changes/` → `archive/`（⏳ 待 CI GREEN 后——未全绿不归档，workflow 阶段 9）

## 备注
- **bootstrap-seed**：本变更 bump zbc/zpkg minor；格式 bump 两代自举由 `fix-bootstrap-format-bump-deadlock`
  CI 机制兜底。**不要与其它格式 bump 踩同一 nightly**（残留窗口期）。
- **JIT interp-first**：v1 JIT 读 flag 但忽略、照常堆分配（准则 1）；gauntlet interp==jit 靠输出等价成立。
- **本地构建救命**：卡了 `rm -rf artifacts/build/compiler && ./xtask build compiler` 冷种重建
  （[[add-compiler-inlining-phase2]] recipe）。
