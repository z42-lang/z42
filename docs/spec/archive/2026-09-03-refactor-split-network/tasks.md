# Tasks: 拆分 corelib/network.rs（refactor-split-network）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（runtime；文本结构阶段 ⑧）
**变更说明：** `corelib/network.rs` 1102 行——922 行是 `#[cfg(not(wasm32))] mod imp { … }` 内联模块。按主题搬到 `network/tcp.rs`
（连接 / 监听 / 收发 / 超时）、`network/tcp_options.rs`（nodelay / ttl / keepalive / 带超时连接 / 带选项监听）、`network/udp.rs`
（UDP / 组播 / DNS / UDP 超时），wasm32 桩到 `network/wasm.rs`；hub 留文档 + `KIND_*` + 句柄槽 + `pub use`，`network::builtin_*`
路径零改动。代码逐行搬移（去一层缩进）。
**原因：** code-organization.md 500 行文件硬限。
**文档影响：** `src/runtime/src/corelib/README.md`（网络行）；`scripts/test/line-limit-baseline.txt`（network.rs 剔除）。

- [x] 1.1 切分 + hub；`cargo check` 0 error（native + wasm32）；`cargo test --lib network` 12 passed
- [x] 2. `xtask test lines --update`（只降）+ `xtask test` GREEN
- [x] 3. 文档同步 + 归档
