# proposal — perf-vm-iteration（VM/stdlib 性能迭代）

状态：🟡 IN-PROGRESS（Phase 0 已落地，2026-07-29）
子系统占用：`runtime`（Phase 1–4）+ `toolchain`(bench harness) + 后续 `stdlib`(Phase 5)

## 动机

当前 interp/jit 性能均需大幅提升。首次建立的 interp/jit 对比（见
[`bench/results/MODE-COMPARISON.md`](../../../../bench/results/MODE-COMPARISON.md)）显示：
调用/虚派发是最贵操作,interp ~467–483ns/(v)call、jit ~102ns/(v)call,**两个引擎都被同一批
锁与分配拖累**。

## 三个根因（跨 interp+jit）

- **A. 每个堆对象自带 `Mutex<T>`**（`gc/region.rs`）——每次 field/array 访问都加解锁 + clone。
- **B. 调用路径锁与分配**——每调用 3 把共享调用栈锁（push/pop/update_line）+ regs Vec + args Vec
  + 2× Arc 帧名分配（即使不抛异常）。JIT 侧 `jit_call` 同样 push/pop + 分配 JitFrame。
- **C. JIT 是"穿线的解释器"**——除数值特化外全 helper call,值不去箱;Cranelift 跑默认 opt 档;
  无分层/OSR。

## 分阶段计划（按 ROI）

| Phase | 内容 | 子系统 | 状态 |
|------|------|--------|------|
| 0 | 对比 harness + 真实基线 | toolchain | ✅ 已落地 |
| 1 | 调用路径去锁去分配（根因 B） | runtime | ⬜ |
| 2 | per-object 锁消除（根因 A / "F1"） | runtime | ⬜ 需 DRAFT |
| 3 | 解释器逐指令微优化 | runtime | ⬜ |
| 4 | JIT 质量（opt_level / safepoint 内联 / field·array 内联 / 去箱） | runtime | ⬜ |
| 5 | 算法级 stdlib（Regex NFA / native str builtin） | stdlib+corelib | ⬜ |

顺序：Phase 1 的「惰性帧名」子项 + Phase 4 的 `opt_level=speed` 是低风险探针,先跑通度量闭环,
再动 Phase 2 高风险机制改造。回归靶：`05_polymorphic_dispatch`（Phase 1/2）、`04_c2_p1_arith_loop`（Phase 4）。

## 度量

每个 Phase 前后跑 `bench/scripts/compare-modes.sh`,更新 `bench/results/mode-comparison.json`
并在本 change 记录 before/after。GREEN 门禁按 workflow 阶段 8:`xtask test` 全绿 + 自举不动点。
