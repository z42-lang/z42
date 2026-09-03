# Tasks: 虚调用目标解析单一实现（unify-vcall-resolution）

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03 | 类型：refactor（runtime）
**变更说明：** 新增 `interp/vcall_resolve.rs`：`resolve_vcall`（装箱基元 → 装箱 struct → 基元/数组 → 对象 vtable /
`resolve_virtual` / 跨包层级 walk 的解析阶梯 + 候选名生成 + PIC 安装）与 `vcall_ic_hit`（PIC 命中）。
`interp/exec_vcall.rs::vcall` 与 `jit/helpers/vcall.rs::jit_vcall` 改为「命中/解析 → 按引擎调用」，各自只保留
调用侧（interp：mixed-mode 原生分流 + `exec_function_from_receiver_regs`；JIT：by-id tiered `FnEntry` 或 interp 回退）。
删除 JIT 侧私有 `resolve_virtual`（对 `module.functions` 线性扫描）。
**原因：** 三面评审 V-9——interp / JIT 对虚调用的接收者分类、候选名、vtable/层级 walk、IC 安装各写一份（488 + 467 行，
注释靠「镜像 interp」人工同步），是自举一致性最大的隐患之一；JIT 副本的 `resolve_virtual` 还是 O(函数数) 线性扫描。
**文档影响：** `src/runtime/src/interp/README.md`（核心文件表）；`docs/book/src/runtime/` 对应 VCall / IC 机制页若存在则补「解析单一实现」。

## 进度概览
- [x] 1. `vcall_resolve.rs`：VCallTarget / ResolvedVCall / vcall_ic_hit / resolve_vcall / resolve_by_candidates
- [x] 2. interp `exec_vcall.rs` 改调用侧；JIT `helpers/vcall.rs` 改调用侧；删 JIT 私有 resolve_virtual
- [x] 3. 验证：`cargo build` + `cargo test --lib` + `xtask test`（interp）+ `xtask test e2e --mode jit` + stdlib jit
- [x] 4. 对比数据：`bench/scenarios/05_polymorphic_dispatch` / `10_mono_vcall` interp+jit 改前/改后（同机 hyperfine）
- [x] 5. 文档同步 + 归档

## 备注
- 行为保持点：候选名顺序按路径参数化保留（基元路径 `{c}.{m}` 先于 `{c}.{m}$arity`，装箱路径相反）；PIC 安装从
  「interp 仅 vtable 命中 / JIT 任一路径」统一为「任一路径解析到模块内函数即安装」（只增不减命中）。
- JIT `Lazy` 目标先经 `resolve_fn_by_name_tiered` 给 JIT 机会注册/编译 lazy slot（保持跨包函数可被 JIT），否则 interp。

## 验证记录（2026-09-03）
- `cargo test --lib` 1040/0；`xtask test` ✅ GREEN 12:16（不动点 3/3）；`xtask test e2e --mode jit` 279/0（+14/0、2/0）；
  `xtask test stdlib --mode jit` 315 文件 / 20 库全过。
- 同机 A/B（`xtask bench --ab --mode both`，base = main 的 z42vm，pr = 本分支 z42vm，同一套 libs/driver，
  hyperfine warmup 3 / runs 10，macOS arm64）：18 组对比全部 `overlap`（无统计显著差异，✅ no regression）。

| 场景 | 模式 | base ms | pr ms | pr/base |
|---|---|---|---|---|
| 05_polymorphic_dispatch | interp | 1110.3 | 1077.3 | 0.970 |
| 05_polymorphic_dispatch | jit | 580.1 | 608.8 | 1.049 |
| 10_mono_vcall | interp | 1987.9 | 1939.7 | 0.976 |
| 10_mono_vcall | jit | 1334.2 | 1359.7 | 1.019 |
| 08_dict_heavy | interp / jit | 66.6 / 65.2 | 65.8 / 63.4 | 0.988 / 0.972 |
| 07_string_heavy | interp / jit | 76.6 / 48.7 | 73.8 / 50.0 | 0.964 / 1.026 |
| 01_fibonacci | interp / jit | 50.7 / 44.1 | 49.0 / 49.2 | 0.967 / 1.116（σ 内）|
| 02 / 03 / 04 / 06 | 两模式 | — | — | 0.917–1.131，皆 overlap |

结论：结构性合一无性能回归；解释器上虚调用密集场景 −2～3%（JIT 副本原 `resolve_virtual` 的线性扫描仅在 IC miss
路径，故 JIT 端无可测差异）。
