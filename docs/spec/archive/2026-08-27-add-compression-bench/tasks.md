# Tasks: add-compression-bench

> 状态：🟢 已完成 | 创建：2026-08-27
> 类型：test（最小化模式）；属「stdlib 性能改善程序」PR5

**变更说明：** z42.compression 此前无 bench 目录（stdlib bench 覆盖最大的洞）。新增
`bench/compression_bench.z42`：Gzip / Deflate / Zstd 对 ~4KB 缓冲的 compress+decompress
round-trip 基准 + 无损 smoke [Test]。
**原因：** 压缩是典型 CPU 热路径，零覆盖 = 回归无法被基准门禁发现。
**文档影响：** 无外部行为变更（纯新增 bench）；bench 目录按约定自动发现，无需改 manifest。

- [x] 1.1 新增 `src/libraries/z42.compression/bench/compression_bench.z42`（gzip/deflate/zstd 4K round-trip + smoke [Test] 验无损）
- [x] 1.2 GREEN 交 PR CI（本机 z42vm wedge）

## 备注
- 零格式 bump（纯新增源码）。
