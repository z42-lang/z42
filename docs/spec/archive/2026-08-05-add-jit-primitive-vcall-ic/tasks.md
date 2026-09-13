# Tasks: JIT primitive-receiver VCall inline cache

> 状态：🟢 已完成 | 创建：2026-08-05 | 完成：2026-08-05
**变更说明：** 让 `jit_vcall` 的 inline-cache 快路径 + 安装也覆盖 primitive 接收者
（string / int / double / char / bool / array），镜像 interp `exec_vcall.rs` 已有的
`value_synthetic_type_id` PIC 逻辑。
**原因：** 现状 JIT 的 VCall IC 只缓存 `Value::Object` 接收者；primitive 接收者每次都
落到 `primitive_class_name` 慢路径，做 `format!`×4 候选名 + `Vec` + 建帧。string-key
Dictionary 的 `key.GetHashCode()` / `keys[slot].Equals(key)` 每次调用都吃这份开销 →
实测 dict jit 比 interp 慢 3.1×、string_heavy 慢 1.2×（object-vcall 场景 jit 快 1.5×，
对照证明差距来自 primitive 派发）。interp 早已有此 IC（refactor-vcall-ic-primitives,
2026-05-17），本变更把 JIT 补齐到对称。
**文档影响：** `docs/book/` JIT 机制页 dispatch 节补一句「primitive 接收者同样走 PIC」；
`src/runtime/src/jit/README.md`（如涉及入口/机制描述）。纯性能、语义不变（结果不变，
仅派发路径变快），无 spec/格式变更。

## 背景数据（一致工具链：nightly .z42 0.4.0 + 本 worktree VM，minor-28）
- dict_heavy: interp 0.10s / jit 0.31s（编译仅 9 方法 / 2.3ms → 慢在执行派发）
- string_heavy: interp 0.25s / jit 0.30s
- 对照 object-vcall：05_polymorphic jit 0.97 vs interp 1.48（快 1.5×）；10_mono_vcall jit 1.54 vs interp 2.27

## 任务
- [x] 1.1 `interp/mod.rs`：`pub(crate) use exec_vcall::value_synthetic_type_id;`
      （与已有 `primitive_class_name` re-export 同处），供 jit helper 引用
- [x] 1.2 `jit/helpers/vcall.rs` IC 快路径（原 `Value::Object` only）：`recv_type` 改为
      Object → `type_desc().id.0`，其余 → `value_synthetic_type_id(other)`；后续 PIC
      lookup / `resolve_fn_by_id_tiered` / 建帧逻辑不变（primitive receiver 也 move 进 callee reg0）
- [x] 1.3 `jit/helpers/vcall.rs` primitive 慢路径（`primitive_class_name` 块）：intra-module
      解析成功后用 `module.func_index.get(func_name)` 取 fn_idx + `value_synthetic_type_id`
      安装 PIC（`vcall_ic_install(ic, synth_id, UNRESOLVED, fn_idx)`），镜像 interp；cross-zpkg
      （lazy_fn）不装
- [x] 1.4 测量（一致工具链：nightly .z42 0.4.0 + worktree VM）：
      **dict_heavy jit 0.31→0.09s（3.4×），追平 interp 0.09**；长 dict(10×) jit 0.40 vs interp 0.48（反超）；
      string_heavy jit 0.30→0.21（反超 interp 0.24）；object-vcall（polymorphic 0.89 / mono）无回退；
      dict 编译仍 9 方法（IC 不增编译）；正确性 interp==jit==24656667 / 247417500
- [x] 1.5 GREEN：`cargo test --lib` **854 passed / 0 failed**（含 jit/vcall/interp 单测）；
      e2e dir-golden 双模式对比 **43/43 interp==jit==expected（0 divergence，interp_only 已尊重）**；
      完整 `xtask test`（interp e2e + cross-zpkg + stdlib + compiler）本地跑；jit 一致性另由 CI
      vm-jit(linux-x64) 专腿最终把关
- [x] 1.6 文档同步：`docs/book/src/runtime/jit-lazy-compile.md`（jit_vcall 派发节加 primitive-IC 说明）
      + `src/runtime/src/jit/README.md`（依赖行加 `value_synthetic_type_id`）

## Out of scope
- Boxed 接收者路径（line ~117）与 `value.rs` to_str 路径：interp 亦不 IC 化 boxed；
  非本次热路径，保持现状
- tiering / 编译税（问题 A）：当前 main 已由 Phase 1c（static-init 跑 interp）解决，本次不碰

## 备注
- 语义不变（identical results），仅 IC 命中省掉 `format!`×4 + `Vec` + 冷解析；
  正确性由「fast path 拿不到（cold/rejected）即回退慢路径」保证，与 object 路径同构。
