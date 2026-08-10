# tasks — perf-vm-iteration

## Phase 0 — 度量地基 ✅（2026-07-29）
- [x] `bench/scripts/compare-modes.sh`：同一 zbc、interp vs jit、hyperfine 测量
- [x] `bench/results/mode-comparison.json` + `MODE-COMPARISON.md`（首次 interp/jit 对比 + 单操作成本）
- [ ] 提交 stdlib micro 基线 JSON（`xtask bench stdlib --json`）——工具已就绪，尚未提交数字
- [ ] 补真实形态宏基准：对象重 / 字符串重 / 混合小解析器（现有场景过合成）

## Phase 1 — 调用路径去锁去分配（根因 B）
> 分析发现:`call_stack` 是 GC root 且有并发标记线程 → 去锁需 GC 并发裁决。见 design.md。
- [ ] **Decision 1（待 User 裁决 A/B/C）**：`call_stack` 锁策略——取决于 GC root 扫描是否 STW 快照
- [x] **Decision 3（安全自主）**：regs Vec 池化（per-thread free-list + Drop for Frame）✅
      → interp: fib 计算 −32%、poly −19%；GREEN 全绿 + 自举 5/5。见 MODE-COMPARISON.md
- [x] **Decision 3（安全自主）**：interp 直接填 callee frame ✅ → `exec_function_from_regs`
      + `Frame::new_from_regs`,直接调用路径消 args Vec + 半 clone。fib 计算 80→68ms
      （累计 −42%）。vcall 路径 prepend `this` 未在 scope,留后续。GREEN 全绿。
- [x] **Decision 3（安全自主）**：vcall 接收者直接填帧 ✅ → `exec_function_from_receiver_regs`
      + `Frame::new_from_receiver_regs`,IC 命中路径消两 Vec + 半 clone。poly 计算
      3804→3247ms（累计 −30%）。GREEN 验证中。
- [ ] interp 帧名：⚠️ 记忆记录 OnceLock 缓存曾 −7%；若做放 boxed FunctionCold + harness 实测（低优先，interp-only）
- [ ] JIT `jit_call` 同步复用（去 per-call JitFrame 分配 + push/pop）——依赖 Decision 1

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
- [x] **Script-First 字符串搜索**：char[]-view 单原语（`__str_to_chars`）+ 脚本 IndexOf（scalar，8.6×）
      —— 见 change `str-scriptfirst-indexof`。剖析否决了 per-op native + 数组 packed 布局（仅 1.35×，
      大头是解释器派发 57× 非布局）；per-char CharAt builtin 派发才是根因，char[]+ArrayGet 吃掉它。
