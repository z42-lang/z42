# Tasks: ambient 守卫 skip-if-same（省 per-frame TLS）

> 状态：🟢 已完成 | 创建：2026-08-18 | 完成：2026-08-18 | 类型：perf/refactor（无可观测行为变化）

**变更说明：** interp/JIT 每帧都装 `HeapGuard`（ambient GC heap）+ `VmGuard`（`CURRENT_VM`）两个
RAII 守卫，各 enter/drop 一次 = 每次函数调用 4 次 thread-local `.with()`（macOS 每次走一次
`_tlv_get_addr`）。但 heap 与 ctx 在整个 call tree 内**恒定**，嵌套帧的 save/restore 是冗余的。
让 `enter` 发现「ambient 已是同一 heap/ctx」（嵌套帧常态）时跳过 store，并让对应 `drop` 跳过
restore（省一次 TLS）。跨 VM 的 native 重入（不同 heap/ctx）仍照常 save+install+restore，行为不变。

**原因：** post-`interp-frame-presize` profile 里 `_tlv_get_addr` 仍是 #4 leaf（~209 samples/~4%）。
call-heavy 前端每帧 4 次 TLS 累积可观。

**实测：** 前端 typecheck（`--dump-bound` big.z42，best of 8）presize 7.20s → skip-if-same 7.15s ≈
**~1.5% faster**；输出逐行 identical。（VmGuard 那半在本 workload 无额外可测收益，但同一优化对称
落地、native-interop 重的场景受益，零成本保留。）

**文档影响：** 无外部行为/机制变更（纯 TLS 访问优化）；守卫语义不变。改动均在 `HeapGuard`/`VmGuard`
的 enter/drop doc 注释里就地说明，无需 book/README 同步。

## 任务
- [x] 1.1 `gc/ambient.rs`：`HeapGuard` 加 `active` 字段；`enter` skip-if-same（`cur == Some(ptr)` → 不 store、drop 不 restore）
- [x] 1.2 `native/exports.rs`：`VmGuard` 同款 skip-if-same（`cur == ptr`）
- [x] 2.1 `cargo build --release`（z42vm）+ `cargo test --lib` + `cargo test --release --tests --no-run`（集成编译）
- [x] 2.2 `xtask test` 完整 GREEN gate 全绿：e2e + cross-zpkg + stdlib + compiler 自举 5/5 gen1==gen2 + vscode-syntax
- [x] 2.3 correctness：dump-bound 输出 identical（已验）+ A/B 复测 ~1.5%（已验）

## 备注
- Deferred/证伪（同批探索，不做）：**杠杆 3 VCall 多态 IC** —— 4-slot PIC 已存在，`IC_SLOTS 4→8`
  实测零收益（7.125 vs 7.138，还略慢），z42c 热站点非 megamorphic + FxHash fallback 已便宜 → 无头子。
  **collect_args 池化**（上一批）实测回归 -1.3%。**drop\<Frame\>** 不可约减。
- 更激进的 `_tlv` 优化（把守卫提到 `run` 顶层装一次，省 enter 的 TLS）留待评估（需 entry-point 审计）。
