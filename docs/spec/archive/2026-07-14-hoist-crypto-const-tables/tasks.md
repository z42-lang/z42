# Tasks: hoist-crypto-const-tables

> 状态：🟢 已完成 | 创建：2026-07-14 | 完成：2026-07-14 | 类型：perf（最小化模式）
> 分支：`claude/hoist-crypto-const-tables`（User 指定新分支）

> **种子已解决（2026-07-14）**：install 脚本硬编码的私有仓库 `codesigner-ui/z42` 本
> session 无权（403），但可见的 `z42-lang/z42` nightly 有完整 SDK 资产
> （`z42-sdk-nightly-linux-x64.tar.gz`，SHA256 校验通过），解到 `.z42/` 即种子。
> 种子 nightly = main@9ab1966（最后代码 commit），到 HEAD 仅两个 docs-only commit、
> 无格式 bump → 种子可直接自建当前源。xtask 经种子 z42c 编成
> （`artifacts/xtask/xtask.zpkg`），驱动方式：
> `Z42_HOME=$PWD/.z42 Z42_LIBS=$PWD/.z42/libs .z42/bin/z42vm artifacts/xtask/xtask.zpkg -- <args>`。

**变更说明：** crypto 全线只读查找表（AES S-box/inv-S-box/Rcon、SHA-256/512 轮常量、
SHA-3 RC+ρ 偏移、BLAKE2b σ+IV、Zip CRC-32 表）从「每块/每轮重建」提为**静态字段**，
经 `__static_init__` 加载期建一次，热路径改 `StaticGet` 读。

**原因：** review §4.2 / 批次 B #1——AES 单块加密重建 S-box 10+ 次、SHA/BLAKE/Keccak
每块重建常量表、Zip CRC 每次调用重建 256 项表；crypto 全线最高性能杠杆。行为完全一致
（表内容不变，仅生命周期从 per-call 变 once）。

**文档影响：** z42.crypto/README 无表级描述、无对外 API 变化 → 无需更新；本 tasks.md 即记录。
（若 z42.compression/README 或 crypto/README 有"每次重建"类描述则同步——实施时核对。）

## 前置：机制验证
- [x] 0.1 确认 z42 静态字段「方法调用初始化器」经 `__static_init__` 加载期执行一次
      —— **已证实**：只改 Sha256 后 `xtask test stdlib z42.crypto` 全绿（27 文件 0 失败），
      数组+方法调用静态初始化器工作，摘要 byte-identical。

## 阶段 1：只读表提静态（read-only，直接别名）
- [x] 1.1 Sha256.z42：`_roundConstants()` → `static long[] _K`；`_processBlock` 用 `Sha256._K`
- [x] 1.2 Sha512.z42：`_roundConstants()` → `static long[] _K`；`_compressBlock` 用 `Sha512._K`
- [x] 1.3 Sha3.z42：`_roundConstants()`/`_rotationOffsets()` → `static long[] _RC` / `static int[] _RHO`；`_keccakF` 用之
- [x] 1.4 Blake2b.z42：`_sigma()` → `static int[] _SIGMA`、`_iv()` → `static long[] _IV`；
      `_compress` 用 `Blake2b._SIGMA` / `Blake2b._IV`（**只读**）；
      `_initHash` **保持** `_iv()` 调用（它 mutate 拷贝，不可别名 static）
- [x] 1.5 Aes.z42：`_sbox()`/`_invSbox()`/`_rconTable()` → `static int[] _SBOX/_INVSBOX/_RCON`；
      `_subBytes`/`_invSubBytes`/`_keyExpansion` 用之
- [x] 1.6 Zip.z42（z42.compression）：`_crc32Table()` → `static long[] _crc32TableStatic`；`_crc32` 用之

## 阶段 2：验证
- [x] 2.1 `xtask test stdlib z42.crypto` —— 27 文件全绿（AES/SHA-2/SHA-3/BLAKE2 向量），
      `xtask test stdlib z42.compression` —— 11 文件全绿（Zip 写读往返即 CRC-32 回归）
- [x] 2.2 完整 GREEN：`xtask test` 全绿（e2e goldens + stdlib 全库 + z42c 自举不动点
      **7/7 byte-identical** + vscode-syntax；crypto 不参与自举，自举面不受影响）
- [x] 2.3 spec 覆盖：向量测试即回归（表内容不变 → 密文/摘要 byte-identical，正确性不变证据）

## 备注
- 只提**只读**表；`_initialHash`（SHA）/ `_initHash`（BLAKE）是**可变运行态种子**，
  保持每次 `new` 拷贝，绝不别名 static（别名会跨调用污染）。
- crypto 不参与 z42c 自举 → 无 byte-identical 自举风险；stdlib 正常重建。
- 子系统锁：短占 `stdlib`（converge-z42c-onto-z42-project 持有），归档即归还——
  User「按你的想法来」授权，沿用批次 A 高危 fix 的短占交接模式。
