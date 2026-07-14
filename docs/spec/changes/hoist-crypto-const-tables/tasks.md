# Tasks: hoist-crypto-const-tables

> 状态：🔴 阻塞（等自举种子）| 创建：2026-07-14 | 类型：perf（最小化模式）

> **阻塞（2026-07-14）**：本环境无法构建 stdlib——fresh checkout 无 warm 产物/无
> committed z42c 种子；冷种子（下载 nightly SDK）被组织 egress 策略封禁
> （`github.com` release-download 路径返回 403，代理 README 禁止绕行/重试）；
> C# bootstrap 已移除无兜底。z42vm 已 cargo 自建
> （`artifacts/build/runtime/release/z42vm`），但无 z42c 种子无法编译 stdlib 跑 crypto
> 向量。**需放行 egress（`github.com` release-download + `*.githubusercontent.com`）
> 或手动投种子到 `.z42/`**，之后按下方阶段实施 + 完整 GREEN 才能提交。
> 代码方案与逐文件改点已在下方就绪，未动任何 crypto 源文件。

**变更说明：** crypto 全线只读查找表（AES S-box/inv-S-box/Rcon、SHA-256/512 轮常量、
SHA-3 RC+ρ 偏移、BLAKE2b σ+IV、Zip CRC-32 表）从「每块/每轮重建」提为**静态字段**，
经 `__static_init__` 加载期建一次，热路径改 `StaticGet` 读。

**原因：** review §4.2 / 批次 B #1——AES 单块加密重建 S-box 10+ 次、SHA/BLAKE/Keccak
每块重建常量表、Zip CRC 每次调用重建 256 项表；crypto 全线最高性能杠杆。行为完全一致
（表内容不变，仅生命周期从 per-call 变 once）。

**文档影响：** z42.crypto/README 无表级描述、无对外 API 变化 → 无需更新；本 tasks.md 即记录。
（若 z42.compression/README 或 crypto/README 有"每次重建"类描述则同步——实施时核对。）

## 前置：机制验证
- [ ] 0.1 确认 z42 静态字段「方法调用初始化器」经 `__static_init__` 加载期执行一次
      （现有先例仅标量字面量 Math.Pi / static int count=0；数组+方法调用未证）
      → 先只改 Sha256 一个文件，`xtask test stdlib z42.crypto` 通过后再铺开

## 阶段 1：只读表提静态（read-only，直接别名）
- [ ] 1.1 Sha256.z42：`_roundConstants()` → `static long[] _K`；`_processBlock` 用 `Sha256._K`
- [ ] 1.2 Sha512.z42：`_roundConstants()` → `static long[] _K`；`_processBlock` 用 `Sha512._K`
- [ ] 1.3 Sha3.z42：`_roundConstants()`/`_rotationOffsets()` → `static long[] _RC` / `static int[] _RHO`；`_keccakF` 用之
- [ ] 1.4 Blake2b.z42：`_sigma()` → `static int[] _SIGMA`、`_iv()` → `static long[] _IV`；
      `_compress` 用 `Blake2b._SIGMA` / `Blake2b._IV`（**只读**）；
      `_initHash` **保持** `_iv()` 调用（它 mutate 拷贝，不可别名 static）
- [ ] 1.5 Aes.z42：`_sbox()`/`_invSbox()`/`_rconTable()` → `static int[] _SBOX/_INVSBOX/_RCON`；
      `_subBytes`/`_invSubBytes`/`_keyExpansion` 用之
- [ ] 1.6 Zip.z42（z42.compression）：`_crc32Table()` → `static long[] _CRC32`；`_crc32` 用之

## 阶段 2：验证
- [ ] 2.1 `xtask test stdlib z42.crypto` —— AES/SHA-2/SHA-3/BLAKE2 向量全绿（正确性不变证据）
- [ ] 2.2 完整 GREEN：`cargo build` + `xtask test`（全 stage）
- [ ] 2.3 spec 覆盖：向量测试即回归（表内容不变 → 密文/摘要 byte-identical）

## 备注
- 只提**只读**表；`_initialHash`（SHA）/ `_initHash`（BLAKE）是**可变运行态种子**，
  保持每次 `new` 拷贝，绝不别名 static（别名会跨调用污染）。
- crypto 不参与 z42c 自举 → 无 byte-identical 自举风险；stdlib 正常重建。
- 子系统锁：短占 `stdlib`（converge-z42c-onto-z42-project 持有），归档即归还——
  User「按你的想法来」授权，沿用批次 A 高危 fix 的短占交接模式。
