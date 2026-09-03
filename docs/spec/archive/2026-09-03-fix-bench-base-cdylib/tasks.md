# Tasks: 修 bench base 侧缺 native cdylib（fix-bench-base-cdylib）

> 状态：🟢 已完成 | 完成：2026-09-03 | 类型：fix（CI；二轮清单 A8）
**变更说明：** `.github/workflows/bench-pr.yml` 的 base 工具链装配里，`src/runtime` 有改动时会 `cargo build --bin z42vm`
建一个 base z42vm，但**没建伴生的 `z42-compression` cdylib**。VM 在自己所在目录解析 `[Native(lib = "z42_compression")]`
（仓库根 `.cargo/config.toml` 的 `target-dir = artifacts/build/runtime`），于是 base-src 的 release 目录里没有 dylib →
compression 的 `[Benchmark]` 全部失败 → `captured compression_bench (partial: some benches failed)` → 采集步非零退出 →
`bench-regression` job 红。修复：同一条命令后追加 cdylib 的 `cargo build`。
**原因：** 该 job 对**每个改 `src/runtime` 的 PR** 都红，需人肉判噪声。实证：#394 / #395 / #402 / #406 / #409 / #413
（`src/runtime` 改动 3–13 个文件）全部红；#404 / #405（0 个 `src/runtime` 文件）不出现此症状。
**文档影响：** 无（workflow 内注释已说明根因）。

- [x] 1.1 `bench-pr.yml` base 侧 cargo build 追加 `z42-compression` cdylib + 根因注释
- [x] 2. 根因验证：按 `src/runtime` 改动数与该 job 结果交叉核对 8 个 PR，完全吻合
- [x] 3. 归档（本 change 不触碰 src/，无需 GREEN；由本 PR 自身的 bench job 验证——它改了 workflow 但没改 src/runtime，
      故 base_vm = pr_vm 路径，真正的验证是**下一个改 src/runtime 的 PR** 不再红）
