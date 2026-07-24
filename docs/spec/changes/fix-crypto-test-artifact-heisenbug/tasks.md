# Tasks: 修复 crypto 测试产物 load-Heisenbug（共享 dist 互删）

> 状态：🟡 进行中 | 创建：2026-07-24 | 类型：fix（最小化模式）

**变更说明：** stdlib 测试 harness 让每个 dir-mode 测试单元的独立 `z42c build` 隔离到自己的
`tests/dm/<unit>/dist/`，不再共享 `tests/dist/`。

**根因（CI 插桩 [CDIAG]/[RDIAG] + 代码 + 结构三重确认）：**
- 数据：`poly1305_vectors.zbc` 编译完 4228 字节存在，run 读前 `-1`（不存在）——**被删除**，非截断/读错。
- 结构：一批并行编译 `[poly1305, rsa, scrypt, secp256k1]`（jobs=4）。**secp256k1 是 dir-mode**（目录，
  走 `z42c build`），其余是单文件（`--emit-zbc` → `.zbc`）。全部写同一个共享 `tests/dist/`。
- 代码：dir-mode build 的 indexed dist 清理 `_cleanOrphanZbc`（`IndexedDist.z42:56-69`）
  `Path.GlobRecursive(整个 dist, "*.zbc")` 后把不属于本次 build 的 `.zbc` 全 `File.Delete`。
- 机制：并行窗口里 poly1305 在 secp256k1 的 cleanup **之前**写好 → 被当孤儿删；rsa/scrypt 在其**之后**
  写（或被既有 retry 重建）→ 幸存。负载相关只是并行时序表象；本质是**多个独立 build 共享输出目录，
  各自的孤儿清理互删兄弟产物**。

**为何选隔离而非改 pack/改 `_cleanOrphanZbc`：** `_cleanOrphanZbc` 对「自己拥有的 dist 清孤儿」是
正确行为；bug 是 harness 的**共享**。隔离 output_dir 从根上消除共享，且无论 build 走 packed/indexed
都不会再互删，比「强制 packed」更稳；纯 harness 改动，无 z42c/bootstrap 顾虑。

**文档影响：** 无对外行为变更（测试基建健壮性）；harness 注释已就地更新。

- [x] 1.1 `scripts/test/xtask_test_lib_units.z42` `_compilePrep` dir-mode：output_dir 由共享 `testsBase`
  改为 per-unit `testsBase/dm/<projName>`；`outArtifact` 同步指向 `dm/<projName>/dist/<projName>.zpkg`
- [ ] 1.2 验证：**CI-only**（load-dependent，本地不可复现）。push → 盯 stdlib-jit/interp 全绿，
  crypto 单元不再 no-artifact/cannot-read；连跑确认稳定

## 备注
- 前两次尝试（原子写 .zbc / GetSize==0 空检查）均已回退——它们没修中根因（文件是被删，非写截断/空）。
  最终定位靠 [CDIAG]/[RDIAG] 插桩直接看到「编译后在、运行前没」。
- 若隔离后仍偶发：说明还有别的共享清理路径，再查（但数据已明确指向 `_cleanOrphanZbc` + 共享 dist）。
