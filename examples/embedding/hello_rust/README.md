# hello_rust — z42 Rust 嵌入示例（Tier 2 ergonomic API）

本 demo 用 `z42-host` Rust crate 演示嵌入。等同 [`../hello_c/`](../hello_c/) 的 Tier 1 C 版本，但 stdout 多了 `[host] ` 前缀（证明走 Rust sink）。

## 跑通

```sh
# 假设你在 SDK package 根目录
cd examples/hello_rust

# Cargo.toml 的 z42-host path-dep 默认指 ../../../src/toolchain/workload/host-api
# 如果你 download SDK package 而不是 clone repo，请改 path-dep 为：
#   z42-host = { git = "https://github.com/z42-lang/z42", path = "src/toolchain/workload/host-api" }
# 或等 Rust crate 上 crates.io（roadmap deferred）

cargo run -- ../hello_c/hello.zbc ../../libs/
# 期望: [host] hello, world
```

## 与 hello_c 对照

[`../hello_c/`](../hello_c/) 走 Tier 1 raw C ABI；本 demo 走 Tier 2 ergonomic Rust API。
两者最终调同一份 `z42_host_*` 函数，stdout 输出一致。Tier 2 处理 Drop guard /
Result-based 错误 / `Arc<dyn ZpkgResolver>` 等细节。

## Roadmap

`z42-host` crate 当前是 path-dep；crates.io publish 进 Phase 4 Deferred backlog。
