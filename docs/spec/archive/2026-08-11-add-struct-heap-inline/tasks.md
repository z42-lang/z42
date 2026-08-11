# Tasks: struct 裸字节内联进堆对象字段 + `struct[]` backing（P3b）

> 顺序 = runtime 地基（惰性）→ 格式 bump wire → codegen 翻转 → GC → 数组 → golden/docs → 归档/PR。
> **前置**：User 审批 Decision D1（a/b/c，design.md §1）与路线 α/β。以下按 **D1-a + α** 编排；选 D1-b 则 GC/写屏障/对象表示任务扩为 unsafe 变体。

## 阶段 0 — 环境 & DRAFT 审批
- [x] 分支 `add-struct-heap-inline` 基于 origin/main `5db42cc6`（warm 环境完好）
- [x] DRAFT 四件（proposal/design/format-delta/tasks）
- [ ] **User 审批 D1 + α/β**（workflow 中断口）

## 阶段 1 — 运行时对象/数组表示（惰性，未接线前 self-host 字节不动）
- [ ] `types.rs:363` `ScriptObject` 加 `struct_bytes: Box<[u8]>` + `struct_refs: Box<[Value]>`（D1-a）
- [ ] `types.rs` TypeDesc/TypeDescCold：class 级内联布局（哪些字段内联 + byte offset + 内联 struct type name → 组合其 struct_layout 得对象级 ref 位图）
- [ ] `arc_heap.rs:1702` `alloc_object`：`struct_bytes`/`struct_refs` 零初始化
- [ ] `exec_object.rs:66` slots 初值：仅遍历「普通字段」子集
- [ ] `Value::StructRef` 变体扩：base 可为堆对象/数组（路线 α），`is_heap_ref`/生命周期正确（指向堆对象须令对象存活）
- [ ] `types.rs:489` `ArrayBacking::StructBytes{elem_size,bytes,refs,layout}`（D1-a）+ `pack_backing`（`:526`）struct 元素落此、`boxed_slice`（`:609`）对其返回 None
- [ ] cargo 单测：对象内联 alloc/字段字节 r/w/整字段 copy 独立性；数组元素 r/w/原地写

## 阶段 2 — GC（核心）
- [ ] `scan_object_refs`（`arc_heap.rs:2005`）/`trace_children`（`types.rs:1000`）：`Object`/`Array` 分支 visit `struct_refs`（D1-a，同 BoxedStruct.refs 一行）
- [ ] 写内联引用叶子 → `struct_refs[k]` Value 槽写 → 复用 `write_barrier_field`（`arc_heap.rs:1836`）
- [ ] cargo 单测：帧退出后堆内联叶子存活；并发 GC 模式（`Z42_GC_MODE=concurrent`）mark/barrier 不漏标

## 阶段 3 — 格式 bump（version-bumping 6/9 步）
- [ ] `ZbcFormat.z42:8` Minor 31→32 + 注释；`ZpkgWriter.z42` Minor 36→37 + 注释
- [ ] `zbc_reader.rs` `ZBC_VERSION_MINOR`/`ZPKG_VERSION_MINOR` 同步 + changelog 注释行
- [ ] 新 flag `CLASS_FLAG_HAS_INLINE_STRUCT=0x08`（`bytecode.rs:124`）
- [ ] `IrModule.z42:72` `IrClassDesc` 加 InlineFieldKinds/Offsets；`ClassDescBuilder.z42:221/228` 填；`ZbcWriter.z42:346` 邻近写内联字段表
- [ ] `zbc_reader.rs:568` 邻近读内联字段表 → 填 TypeDesc
- [ ] `docs/design/runtime/zbc.md` + `zpkg.md` changelog 各一行
- [ ] fixture 重生（`xtask build test` 自动 zbc 6；手工 zpkg 4）+ golden hex 重截（`zbc_tests.z42`）
- [ ] `cargo test --test zbc_compat` + `cargo test lazy_loader` 绿

## 阶段 4 — 编译器 codegen 翻转
- [ ] `ExprEmitter.z42:700/753`：`obj.f` 当**字段类型 IsBlobStruct** → 发内联地址句柄 + 叶子 prim 访问（路线 α，复用 StructFieldGetPrim/SetPrim）
- [ ] `c.pt = other` 整字段 → `StructCopy`（堆对象 base）复用 `_copyRegion`
- [ ] `arr[i]` / `arr[i].x=v` 数组 base 地址句柄 + 叶子直写（3a）
- [ ] `CompilerFingerprint++`（若本 change 未额外 bump 已由格式 bump 失效 cache，可省——见 version-bumping §指纹）
- [ ] `--dump-ir` 验内联字段访问 IR 正确；IrOptInfo def/use 录入（防 DCE 误删喂值，参 A-use 踩坑）

## 阶段 5 — e2e / docs
- [ ] golden `src/tests/types/struct_heap_inline.z42`：class struct 字段 r/w + 复制独立性 + string 叶子 + `Point[]` 原地可变 + 装箱往返；负控制
- [ ] `codegen_tests.z42` 内联字段 IrDump 对比
- [ ] `docs/book/src/runtime/struct-value-semantics.md` 加「堆内联 + 数组 backing + GC 写屏障」节（机制原理 + mermaid，doc-system 复杂实现原理规则）
- [ ] `docs/roadmap.md` P3b 标记完成、P4/P5 更新；`docs/features.md` 登记
- [ ] 更新 memory `packed-primitive-arrays`（inline struct[] 收敛落地）

## 阶段 6 — GREEN / 归档 / PR
- [ ] `cargo test --lib` + `xtask test`（**不传 Z42_HOME**）+ self-host 5/5 gen1==gen2
- [ ] `xtask test bootstrap`（上一 nightly 编当前源 → 无越界；格式 bump 两阶段纪律）
- [ ] 归档 `docs/spec/archive/2026-08-11-add-struct-heap-inline/`
- [ ] rebase origin/main + 重跑 GREEN → PR（body 三段 + 页脚）→ 盯 CI 两代自举吸收 bump → User 手动合（分支保护，见 [[stale-required-check-blocks-pr-merge]]）→ 删分支/worktree
