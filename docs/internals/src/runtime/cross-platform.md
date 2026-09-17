# Cross-Platform 构建

> 状态：P4.1 已落地（feature flags），P4.2/P4.3/P4.4（wasm/iOS/Android 实际脚手架）待实施。

z42 VM 同一份 Rust 代码通过 Cargo features 构建出适合不同平台的产物。`src/runtime/Cargo.toml` `[features]` 段定义可组合的 feature 集合。

## Features 清单

| Feature | 说明 | 默认 |
|---------|------|------|
| `jit` | Cranelift JIT backend（桌面 x64/aarch64 专属） | ✅ default |
| `aot` | AOT 编译占位（M9 LLVM/inkwell；当前无实现） | — |
| `interp-only` | 仅 interpreter，不引入 jit / aot；意图标识 | — |
| `wasm` | = `["interp-only"]`（wasm 沙箱禁动态代码生成） | — |
| `ios` | = `["interp-only", "aot"]`（iOS App Store 政策禁 JIT） | — |
| `android` | = `["interp-only", "aot"]` | — |

## 构建矩阵

```bash
# 默认（含 JIT，桌面开发用）
cargo build --manifest-path src/runtime/Cargo.toml

# 仅 interpreter（轻量产物，无 cranelift）
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features interp-only

# 平台 preset（host target；实际 cross-compile 由 P4.2/P4.3/P4.4 接入）
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features wasm
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features ios
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features android
```

`./xtask build feature-matrix` 一次性跑完 4 个 preset 验证。CI `feature-matrix` job 锁定。

## CLI 行为差异

`z42vm --help` 的 `--mode` 选项根据编译时启用的 feature 自动调整：

```
# 默认编译
--mode <MODE>    [possible values: interp, jit]

# --features interp-only
--mode <MODE>    [possible values: interp]
```

用户在 interp-only 产物上传 `--mode jit` → clap 直接报错 `invalid value 'jit' for '--mode'`，不会走 runtime check。

## 源码 cfg gate 规则

- `src/lib.rs` `pub mod jit` / `pub mod aot` 加 `#[cfg(feature = "...")]`
- `vm.rs` 的 `ExecMode::Jit => crate::jit::run(...)` 调用点加 `#[cfg(feature = "jit")]`；feature off 时该 arm 改 bail 友好错误
- `metadata::ExecMode` enum 的所有 variant 都保留（zbc 文件可能编码任何 mode；无法解码就 fallback Interp）
- main.rs CLI 的 `enum ExecMode` 与 `metadata::ExecMode` 不同 —— CLI 端 variant 加 cfg

## 互斥不强制

Cargo features 设计上是 additive。`compile_error!` 互斥检查与 cargo 哲学冲突。开发者选错组合（例如 `wasm + jit`） → 由 CI feature-matrix job 暴露，而不是源码强制。

## 后续阶段

- **P4.2 add-platform-wasm**：wasm-bindgen + npm package + 浏览器 / Node.js demo
- **P4.3 add-platform-android**：Gradle AAR + JNI bridge + Kotlin facade
- **P4.4 add-platform-ios**：SwiftPM Package + xcframework + SwiftUI demo

每个 P4.x 在本 spec 之上叠加：选合适的 feature preset、加 host bridge、加 demo / e2e 测试。

> **跨平台测试架构**（同一份 `src/tests/` 在所有平台一致执行）见
> [cross-platform-testing.md](../../../internals/src/testing/cross-platform.md) — library-first
> test-runner + zbc 一次编译 N 处分发 + `[SkipPlatform]` 选择性跳过。
