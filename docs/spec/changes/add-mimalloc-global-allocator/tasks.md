# Tasks: mimalloc 全局分配器（desktop z42vm）

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06
**变更说明：** 给 z42vm 二进制接入 mimalloc 作为 `#[global_allocator]`，经默认开启的
`mimalloc-alloc` feature 门控（wasm/ios/android 预设 `--no-default-features` 天然不含）。
**原因：** profile 实测 z42c 自编译**分配受限**——`libsystem_malloc` 占 ~31% 采样、
`--mode jit` 与 `--mode interp` 几乎同速（22.1 vs 22.4s，因 tiering 让编译主要跑 interp、
且分配不因 JIT 变快）。换 mimalloc 后 **z42c 编译 −40%/−44%（interp 13.2s / jit 12.5s）、
StringHeavy ~3×（0.24→0.07）**，计算/vcall 型负载中性（符合预期）。interp 与 jit 全受益。
**文档影响：** `src/runtime/README.md`（若列依赖/feature）；`docs/book/` 运行时构建/性能页记一句
「desktop 默认 mimalloc」；`docs/workflow/`（打包矩阵不受影响，因 preset 已 --no-default-features）。

## 背景数据（一致 0.34 工具链，本地 release VM）
| workload | 系统 malloc | mimalloc | Δ |
|---|---|---|---|
| z42c 编译 big.z42（400 类）interp | 22.1s | 13.2s | −40% |
| z42c 编译 big.z42 jit | 22.4s | 12.5s | −44% |
| StringHeavy interp/jit | 0.24/0.22 | 0.07/0.07 | ~3× |
| PolymorphicDispatch（计算/vcall） | 1.41/0.89 | 1.48/1.01 | 中性 |

## 任务
- [x] 1.1 `src/runtime/Cargo.toml`：`mimalloc = { version="0.1", optional=true }`；
      feature `mimalloc-alloc = ["dep:mimalloc"]`；加入 `default`
- [x] 1.2 `src/runtime/src/main.rs`：`#[cfg(feature="mimalloc-alloc")] #[global_allocator]`
- [x] 1.3 跨平台门控验证：`--no-default-features --features interp-only`（wasm/mobile 同类）
      编译通过且 `cargo tree` 不含 mimalloc
- [x] 1.4 GREEN：`cargo build --release`（默认，含 mimalloc）+ `cargo test --lib` 全过；
      `xtask test` e2e/stdlib/compiler（分配器不改语义，结果不变）
- [x] 1.5 文档同步：README/book 记「desktop 默认 mimalloc」+ 本 profile 依据

## Out of scope
- wasm/ios/android 采用 mimalloc（C 构建不入 wasm 沙箱 / 移动端体积敏感；preset 已排除）
- 嵌入式 lib 场景（`#[global_allocator]` 仅作用于 z42vm 二进制；宿主用自身分配器）
- 减少分配**量**（try_lookup_function HashSet clone 等）——独立 change，与本次正交

## 备注
- 语义不变（仅换分配器实现）；正确性由 cargo test + e2e 覆盖。
- mimalloc 从 C 源构建，需目标平台 C 工具链（desktop 均具备；CI 桌面腿已有）。
