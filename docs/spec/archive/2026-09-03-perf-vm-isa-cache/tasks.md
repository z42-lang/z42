# Tasks: 类型判定身份缓存（perf-vm-isa-cache）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：perf（runtime；三面评审 V-6）
**变更说明：** `is` / `as` / 带类型 `catch` 的判定在既有字符串 memo（`subclass_memo`：锁 + 两次 FQ 名哈希）
前面加一层 per-VmContext **身份键**直接映射缓存 `IsaCache`（键 = 接收者 `*const TypeDesc` + 目标类名串地址，
命中 = 两次 relaxed load）；interp / JIT / catch 三处统一走 `dispatch::isa_td`，删除 JIT 私有的重复链式遍历
（`is_subclass_or_eq_walk` / `iface_reaches_mod`）。
**原因：** z42c 序列化每条 IR 指令走 ~60 路 `is` 链，memo 命中仍 ~60 ns/次，是 z42c（JIT 下运行）的热点；
且 interp 与 JIT 各维护一份等价的基链/接口遍历。
**文档影响：** `src/runtime/src/vm_context/README.md`（核心文件 + 功能索引）、`src/runtime/src/interp/README.md`、
`src/runtime/src/jit/README.md`（helpers 不再自带 walk）、`docs/book` 对象/类型判定机制页（若有 optimize-subclass-check 段则追加）、
`bench/README.md`（新场景 11）。

- [x] 1.1 `vm_context/isa_cache.rs`（+ `_tests.rs`）：1024 槽直接映射，`get/put/clear`；`types.rs` 字段、`construct.rs` 两处初始化、
      `lookup.rs` 两处显式 (re)load 与 memo 同步清空
- [x] 1.2 `interp/dispatch.rs::isa_td`：`id == UNRESOLVED`（临时 fallback 描述符）不缓存；其余先查缓存再落 memo
- [x] 1.3 interp 调用点：`exec_object.rs` is_instance / as_cast（4 处）、`interp/mod.rs::find_handler`（去掉每次 throw 的类名 String clone）
- [x] 1.4 JIT 调用点：`jit/helpers/object.rs` is_instance / as_cast（4 处）改走 `isa_td`，删私有 walk；`jit/helpers/control.rs` catch 判定
- [x] 1.5 bench 场景 `bench/scenarios/11_type_test_chain.z42`（8 路 is 链 + 接口 is + as；期望输出 39000000）
- [x] 2. 对比数据：`xtask bench --ab --mode both`（base = main 9b4ac4a5 VM，同一 zbc）；z42c 自编译计时（z42c.semantics 包，`--mode jit`）
- [x] 3. `cargo test --lib isa_cache` + `xtask test` GREEN + `xtask test e2e --mode jit`
- [x] 4. 文档同步 + 归档

## 备注
- 键用**地址**而非内容：目标名只允许来自不朽元数据（指令 / 异常表 / JIT 烘焙同一串）；反射路径（`IsAssignableFrom` 等）
  仍走 `is_subclass_or_eq_td`（字符串 memo），不进本缓存。
- 直接映射、冲突覆盖、单写者（VmContext 属单一 mutator 线程）；原子只为 `Sync`，非跨线程协议。

## 对比数据（2026-09-03，macOS arm64 同机；base = main 9b4ac4a5 z42vm，pr = 本分支 z42vm；同一 zbc / 同一 libs / 同一 driver）

`xtask bench --ab --mode both`（hyperfine warmup 3 / runs 10，ratio = pr/base，CI 重叠即 overlap）：

| 场景 | interp base→pr (ms) | ratio | jit base→pr (ms) | ratio | 判定 |
|---|---|---|---|---|---|
| **11_type_test_chain**（新增：8 路 is 链 + 接口 is + as）| 1485.8 → 1025.6 | **0.690（1.45×）** | 1078.5 → 631.5 | **0.586（1.71×）** | ↓ faster / ↓ faster |
| 05_polymorphic_dispatch | 1100.4 → 1104.5 | 1.004 | 579.8 → 592.3 | 1.022 | overlap |
| 10_mono_vcall | 1944.8 → 1936.8 | 0.996 | 1327.1 → 1343.8 | 1.013 | overlap |
| 04_c2_p1_arith_loop | 198.0 → 196.6 | 0.993 | 69.6 → 69.3 | 0.996 | overlap |
| 01 / 02 / 03 / 06 / 07 / 08（30–85 ms 短基准）| — | 1.01–1.05 | — | 1.02–1.09 | overlap（CI 内；不触及 is/as 路径）|

z42c 自编译（`z42c.driver --mode jit -- build z42c.semantics.z42.toml --release`，无增量缓存，base/pr 交替 3 轮）：
12.458 / 12.446 / 12.463 s → 12.243 / 12.013 / 12.155 s，**−2.5%**（TypeChecker 的 is 链只占编译总时的一小部分；序列化阶段的收益见场景 11）。
