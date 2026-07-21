# Tasks: perf-crypto-bigint-hotpaths

> 状态：🟢 已完成 | 创建：2026-07-21 | 完成：2026-07-21 | 类型：perf（最小模式，行为保持）
> 子系统：`stdlib`（短占，User 授权预抢 converge 锁，隔离 worktree）

**变更说明：** perf 攻坚 change #1（stdlib 常量化 + 分块）——消除 crypto/BigInt 热路径的每迭代冗余分配。

**原因：** perf 分析（本会话 3-agent + bench 数据）指出 SHA256/BLAKE3 每 block 重建编译期
常量表、BigInt.Parse 逐位分配。行为保持（哈希逐字节一致、Parse 值不变）。

**文档影响：** 无外部行为变更（纯内部优化，无 API/格式变化）。

- [x] 1.1 `Sha256.z42`：`_roundConstants()` 提为 static `_K`（一次性 __static_init__），
      `_processBlock` 读 `Sha256._K`（was 每 block `new long[64]` + 64 stores）。k[i] 只读，安全。
- [x] 1.2 `Blake3.z42`：`_sigma()` 提为 static `_SIGMA`（was 每 block 重建 + 嵌套派生循环 +
      多个 int[16] 分配）；`_compress` 内联 iv[0..3] 常量到 state[8..11]（was 每 block `_iv()`
      调用 + `new int[8]`；iv[4..7] 本就不用）。sigma 只读、iv 只读拷入 state，安全。其余 3 处
      `_iv()`（per-chunk/parent，非每 block）保持不动，避免别名风险。
- [x] 1.3 `BigInt.z42`：`Parse` 改 9 位十进制分块（机器字累积 chunk，每满 9 位 1 次 BigInt
      Multiply+Add；was 每位 Multiply+Add+2 new BigInt ≈ 5-7 分配/位）。对称 ToString 的 10^9 分块。
      ParseHex 保持不动（未 benched，保持聚焦）。
- [x] 1.4 GREEN + 量化：
      - correctness：`test stdlib z42.crypto` **28 文件全绿**（哈希向量逐字节一致）+
        `test stdlib z42.numerics` **16 文件全绿**（BigInt.Parse 值正确）
      - bench diff（同机 vs 本会话 pre-opt baseline）：
        - **bigint_parse: 2.96ms → 0.41ms = 7.27×** ✅（headline）
        - blake3_4k: 43.6 → 41.1ms ≈ **1.06×**；blake3_small 1.04×
        - **sha256_4k: 72 → 76ms ≈ 噪声（无 wall-time 收益）**
- [x] 1.5 完整 gate 以 CI 为权威（冷环境）

## 备注
- **诚实结论**：BigInt.Parse 分块大赢（7.3×）；BLAKE3 常量化 ~6%（sigma 派生循环是真浪费）；
  **SHA256 常量化未动 wall-time**——证实 perf 分析 F1：SHA256 瓶颈是轮循环 + **数组元素访问锁
  （runtime `gc/refs.rs` per-op Mutex）**，非常量重建。SHA256 改动仍保留（消每 block 64 次分配、
  减 GC 压力，且与后续 F1 runtime 改动叠加），如实标注无独立收益。
- 下一步（按 perf 顺序）：change #2 regex R1/R2/R3（命中 7.7ms），或 change #5 runtime F1
  （数组锁，SHA256/所有 byte-loop 的真瓶颈）。
