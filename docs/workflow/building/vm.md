# VM 构建（Rust）

z42 VM (`z42vm`) 是 Rust crate，源码在 [`src/runtime/`](../../../src/runtime/)。

## 前置

- Rust stable（最新版本）
- macOS / Linux / Windows 任一

## 标准构建

```bash
cargo build --manifest-path src/runtime/Cargo.toml              # Debug
cargo build --manifest-path src/runtime/Cargo.toml --release    # Release
```

产物：`artifacts/build/runtime/{debug,release}/z42vm`（binary）+ `libz42_vm.{rlib,dylib,so,dll}`（library）。

## 运行 VM

```bash
cargo run --manifest-path src/runtime/Cargo.toml -- <file.zbc | file.zpkg> [--mode interp|jit|aot]
```

或用分发版 binary（先 `./xtask package sdk --profile debug`）：

```bash
./artifacts/build/runtime/release/z42vm <file.zbc>
```

VM 通过文件扩展名分发：`.zbc` 走 `load_zbc`，`.zpkg` 走 `load_zpkg`（[`src/runtime/src/metadata/loader.rs::load_artifact`](../../../src/runtime/src/metadata/loader.rs)）。

## 执行模式

`--mode` 默认值由 zbc 的命名空间级 `[ExecMode]` 注解决定；命令行 flag 强制覆盖。详见 [`docs/design/runtime/execution-model.md`](../../design/runtime/execution-model.md)。

| 模式 | 说明 |
|------|------|
| `interp` | 直接 fetch-decode-dispatch（启动最快、最便携） |
| `jit` | Cranelift JIT（热函数原生码、interp 启动后切换） |
| `aot` | LLVM AOT（**M9 未实施**；当前 `bail!`） |

## Feature flags

VM 支持以下 cargo feature：

| Feature | 默认 | 用途 |
|---------|:----:|------|
| `interp` | ✅ | 启用解释器（必有）|
| `jit` | ✅ | 启用 Cranelift JIT |
| `host` | ✅ | 启用 Host C ABI（embedding）|
| `native-interop` | ✅ | 启用 native 类型注册（C2-C11 spec）|

构建 interp-only 子集（嵌入式 / 最小测试）：

```bash
cargo build --manifest-path src/runtime/Cargo.toml --no-default-features --features interp
```

## 调试 build

split-debug-symbols 机制（zbc / zpkg sidecar `.zsym`）见 [`docs/design/runtime/vm-architecture.md`](../../design/runtime/vm-architecture.md) "Sidecar 调试符号加载" 段。运行时 VM 自动探测同目录 `.zsym` 并按 build_id 配对合并；trace 内含源码 `file:line:col`。

## 测试

见 [`../testing/vm-tests.md`](../testing/vm-tests.md)。
