# Tasks: merge-hostrun-into-apphost

> 状态：🟢 已完成 | 完成：2026-08-31 | 类型：refactor（最小化模式）

**变更说明：** 删除独立的 `z42-hostrun` crate，把其"定位已部署 z42vm + libs 并子进程 exec app zpkg"
（apphost run 路径，纯 std、零 runtime 依赖）的逻辑折叠进它唯一的 Rust 消费者——桌面 apphost 桩，
作为其 `hostrun` 模块（`src/hostrun.rs`）。

**原因：** 当初抽出 `z42-hostrun` 成独立 crate，是为了让 apphost 桩与 **Rust launcher trampoline**
共享一份实现、又不让 workload 依赖 launcher crate。但 launcher 此后已用 z42 重写
（`src/toolchain/launcher/core/*.z42`），那个 Rust trampoline crate 不再存在——`z42-hostrun` 的唯一
Rust 消费者只剩 apphost 桩。"共享"前提消失后，独立成 crate 只剩过度拆分；并回桩里更简单、且清掉那条
已过时误导的"shared with launcher trampoline"注释。

**为什么不并进 z42-host（区别于本次）：** z42-host 是**进程内嵌入**（link runtime crate），hostrun 是
**进程外拉起**（纯 std、不 link VM）；二者是相反执行模型，合并会把 runtime 强塞进 100KB apphost 桩
或反之。本次只是把"进程外拉起"逻辑从独立 crate 收回它唯一的使用者，不跨执行模型。

**文档影响：** `src/runtime/crates/README.md`、`src/runtime/README.md`、`src/toolchain/launcher/README.md`、
`docs/design/runtime/launcher.md`、`docs/design/testing/embedded-app-run.md`（均改 `z42-hostrun` crate 引用
→ apphost `hostrun` 模块）。roadmap R8a「host/hostrun/main 统一」是更大目标，本次为其一步，roadmap 不改。

- [x] 1.1 新建 `apphost/src/hostrun.rs`：迁入 `z42-hostrun/src/lib.rs` 全部内容（含 17 单测），更新头注释
- [x] 1.2 `apphost/src/main.rs`：`use z42_hostrun::{…}` → `mod hostrun; use hostrun::{…}`
- [x] 1.3 `apphost/Cargo.toml`：删 `z42-hostrun` path 依赖；更新头注释
- [x] 1.4 删除 `src/runtime/crates/z42-hostrun/`（整目录）
- [x] 1.5 `src/runtime/Cargo.toml`：workspace members 移除 `crates/z42-hostrun`
- [x] 1.6 文档同步（5 处 README/design 的 crate 引用改为 apphost `hostrun` 模块）
- [x] 1.7 验证：apphost `cargo test --release`（17 passed）+ runtime `cargo build --release`（z42vm 干净构建）

## 备注

纯重构，不改任何运行时行为：apphost 二进制的解析/exec 逻辑逐字保留（含全部单测），只是从独立 crate
变为同 crate 内的模块。apphost 是独立 Cargo workspace（自带 Cargo.lock），经 path 依赖引 hostrun；
合并后 path 依赖消除，runtime workspace 少一个 member。
