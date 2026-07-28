# tasks — perf-vm-iteration

## Phase 0 — 度量地基 ✅（2026-07-29）
- [x] `bench/scripts/compare-modes.sh`：同一 zbc、interp vs jit、hyperfine 测量
- [x] `bench/results/mode-comparison.json` + `MODE-COMPARISON.md`（首次 interp/jit 对比 + 单操作成本）
- [ ] 提交 stdlib micro 基线 JSON（`xtask bench stdlib --json`）——工具已就绪，尚未提交数字
- [ ] 补真实形态宏基准：对象重 / 字符串重 / 混合小解析器（现有场景过合成）

## Phase 1 — 调用路径去锁去分配（根因 B）
- [ ] 惰性帧名/文件名：只存 func 引用 + line，异常展开时才 format（省 2× Arc/调用）【先做，探针】
- [ ] 调用栈改 per-VM-thread 无锁结构（消除 push/pop/update_line 3 把锁）
- [ ] regs Vec 池化 / frame arena
- [ ] `collect_args` 改传 slice 免分配
- [ ] JIT `jit_call` 同步复用（去 per-call JitFrame 分配 + push/pop）

## Phase 2 — per-object 锁消除（根因 A / F1）— 需 DRAFT 设计
- [ ] 设计与 GC safepoint 协调的单线程无锁快路（仅并发 mark 时 park）
- [ ] 扩 `data_ptr_unlocked` 思路到可变槽 + 写屏障边界确认

## Phase 3 — 解释器逐指令微优化
- [ ] `site_idx` 解析移出热 arm
- [ ] safepoint 降频（计数批处理）
- [ ] 热操作 borrow 语义、减 `frame.get` 的 Result/clone

## Phase 4 — JIT 质量（根因 C）
- [x] `opt_level=speed`（一行，先测）→ **负收益,已保留默认档**（零计算提升 +4-5ms 冷编译；
      根因:helper call + 24B load/store Cranelift 无法跨 op 去箱。见 MODE-COMPARISON.md）
- [ ] safepoint 快路内联
- [ ] 单态 FieldGet/Set + Array 内联
- [ ] Div/Rem 内联 + 零检查冷边
- [ ] i64/f64 跨表达式去箱（SSA 寄存器驻留）

## Phase 5 — 算法级 stdlib（可与 runtime 并行的纯 Regex 部分）
- [ ] Regex Thompson-NFA 引擎
- [ ] native `__str_join` / StringBuilder builtin
