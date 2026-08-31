# crates/

## 职责

`z42`（VM 主 crate，`src/`）之外的 Rust workspace 子 crate。分两组：

1. **native 类型创作 SDK**（`z42-abi` / `z42-rs` / `z42-macros`）——**刻意压在 VM 之下、不依赖
   runtime crate**，靠稳定 `extern "C"` 符号回连 VM。因此**两类消费者共用同一套**：
   - 被运行中的 `z42vm` `dlopen` 的独立 native 扩展（如 `numz42-rs`）；
   - **嵌入式应用**——link 了 `z42-host` 把 VM 嵌进自己进程、又想把自己的 Rust 类型注册进这台
     内嵌 VM，用的也是这套 `Z42Type` / `#[z42::methods]` / `module!`。

   > 它们**不是"仅扩展侧 / 仅 Tier 2"**；跟"独立扩展还是嵌入宿主"无关，只要"创作暴露给 z42 的
   > native 类型"就用这套。保持 runtime-free 正是为了让这两类消费者都能用而不拖进整台 VM。

2. **驱动 / 运行 VM 侧**（`z42-host`）——在 VM 之上：`z42-host` link runtime crate 在**进程内**
   嵌入执行（mobile/wasm workload、桌面嵌入宿主用）。

此外 `z42-compression` / `z42-repl` 是纯 C ABI 的 native 扩展 cdylib（被 `z42vm` dlopen，与主 crate 无 Cargo 环）。

> **进程外拉起（out-of-process）不在本目录**：桌面 apphost 那条"定位已部署 `z42vm` + libs 并
> **子进程** exec app zpkg"的逻辑（曾是独立 `z42-hostrun` crate）已折叠进
> [`src/toolchain/workload/desktop/platform/apphost/src/hostrun.rs`](../../toolchain/workload/desktop/platform/apphost/src/hostrun.rs)
> ——它纯 std、不 link VM，唯一消费者就是那个 apphost 桩，故住在桩里而非这里
> （merge-hostrun-into-apphost；此前的 launcher 共享者已用 z42 重写）。

## 功能索引

| 子目录 | 包名 | 层 | 职责 |
|--------|------|-----|------|
| `z42-abi/` | `z42-abi` | 之下 · Tier 1 | C ABI 的 Rust `#[repr(C)]` 冻结布局镜像（`Z42Value` / `Z42_ABI_VERSION` …）；`#![no_std]`、零依赖 |
| `z42-rs/` | `z42-rs` | 之下 · Tier 2 | native 类型创作的人体工学层：`Z42Type` / `Z42Traceable` / `Visitor` trait + 类型别名；re-export `z42-macros`。需 std，不依赖 runtime |
| `z42-macros/` | `z42-macros` | 之下 · Tier 2 | proc-macro：`Z42Type` derive、`methods` / `trait_impl` attr、`module!`。**独立编译产物（Rust 规则），不可并入普通 lib** |
| `z42-host/` | `z42-host` | 之上 | 宿主**进程内嵌入** API：`Result` 错误 / `Drop` 清理 / `Box<dyn Fn>` sink；link runtime crate。包在 Tier 1 C ABI（`z42::host`）外的 Tier 2 |
| `z42-compression/` | `z42-compression` | 旁 · native 扩展 | 压缩后端（gzip/zlib/deflate/zstd，`z42.compression` 的 native 侧）；cdylib+staticlib+rlib，desktop/mobile dlopen、wasm 静态链 |
| `z42-repl/` | `z42-repl` | 旁 · native 扩展 | **host-only** 交互式 REPL（`z42i`）行编辑器，wrap rustyline；`z42vm` 首次 `__repl_readline` 时 dlopen。VM 重入（补全/按键编辑）经 `ReplCallbacks` 的 C 函数指针回跨，无 z42 内部类型过边界。产物随 `z42i` 放 toolchain/interactive 目录，**不进** `<sdk>/native/`；wasm/mobile 走纯 stdin fallback |

## 依赖关系

```
        独立 native 扩展 (.so)        嵌入式应用 (host)
                │                          │
                │        ┌─────────────────┤
                ▼        ▼                 ▼
              z42-rs ──→ z42-abi ◀──── z42-host ──→ z42 (runtime crate)
                │            ▲                             │
                ▼            │                             ▼
            z42-macros ──────┘                          z42-abi
             (展开引用 abi 类型)

z42 (runtime) ──→ z42-abi (实现 ABI 函数)        z42vm --dlopen--> z42-compression / z42-repl

进程外拉起（不在本 workspace）: desktop/…/apphost 桩 —(exec 子进程)→ 已装的 z42vm
```

要点：`z42-abi` 是所有人（含 runtime 本体、wasm platform、host）共享的 no_std 地基；`z42-rs` /
`z42-macros` 停在 VM 之下（不依赖 runtime），`z42-host` 在 VM 之上（依赖 runtime）。**这条
below/above-VM 线 + proc-macro 编译约束，就是这些 crate 不能互相合并的原因**（并 `abi`/`rs` 进
`host` 会成依赖环 / 把 runtime 强塞给独立扩展 / 丢 no_std；并 `macros` 违反 proc-macro 规则）。

## 如何测试验证

```bash
cargo test --manifest-path src/runtime/Cargo.toml -p z42-abi     # ABI 布局不变量
cargo test --manifest-path src/runtime/Cargo.toml -p z42-macros  # trybuild 宏展开（含 fail 用例）
cargo test --manifest-path src/runtime/Cargo.toml -p z42-rs      # skeleton trait
cargo test --manifest-path src/runtime/Cargo.toml -p z42-repl    # 行编辑器单测
# native 扩展端到端（dlopen 路径）随 VM goldens 覆盖：
xtask test e2e
```

## 关联文档

- 嵌入 API（Tier 1 C ABI + Tier 2）：[`docs/design/runtime/embedding.md`](../../../docs/design/runtime/embedding.md)
- native interop ABI（Tier 1/2 分层）：spec `design-interop-interfaces` 及 C2–C5 后续
- REPL native 剥离：change `extract-repl-native-cdylib`；`}` 回退 floor：`add-repl-rbrace-floor`（均在 `docs/spec/changes/`）
- apphost run 路径（进程外拉起）：change `add-apphost` / `simplify-apphost-direct-run`（`docs/spec/archive/`）；实现已并入 apphost 的 `hostrun` 模块（change `merge-hostrun-into-apphost`）

## 状态

`z42-abi` / `z42-macros` / `z42-host` / `z42-compression` / `z42-repl` 均为在用产物；
`z42-rs` 仍是 C1 接口骨架（trait 形状稳定，运行时行为 / derive 实现待 C2–C5 填入）。
