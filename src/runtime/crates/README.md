# crates/

## 职责

`z42_vm` 之外的 Rust workspace 子 crate。所有 z42 native interop 的 Rust 侧公开接口住在这里：ABI 类型镜像、用户 trait/type、proc macro。

`z42_vm` 通过 path 依赖引用这些 crate；用户 native 库（如 `numz42-rs`）也以 git/path 依赖方式引用同一组 crate。

## 核心子目录

| 子目录 | 包名 | 职责 |
|--------|------|------|
| `z42-abi/` | `z42-abi` | Tier 1 C ABI 的 Rust `#[repr(C)]` 镜像；`no_std`，无依赖 |
| `z42-rs/` | `z42-rs` | Tier 2 用户面向 trait（`Z42Type`、`Z42Traceable`、`Visitor`）+ 类型别名 |
| `z42-macros/` | `z42-macros` | Tier 2 proc macro：`Z42Type` derive、`methods`/`trait_impl` attr、`module!` |
| `z42-host/` | `z42-host` | 宿主嵌入 API（workload host facade，原 `toolchain/workload/host-api` 迁入） |
| `z42-hostrun/` | `z42-hostrun` | 宿主运行时驱动（apphost / 嵌入执行入口） |
| `z42-compression/` | `z42-compression` | 压缩后端（`z42.compression` 的 native 侧） |

## 依赖关系

```
用户 native 库
    │
    ▼
z42-rs ──→ z42-abi
    │           ▲
    ▼           │
z42-macros ─────┘ (展开时引用 z42-abi 类型)

z42_vm ──→ z42-abi (实现 ABI 函数)
```

## 状态

C1 接口骨架。详见各子 crate 的 README。运行时行为与 derive macro 实现在 C2–C5 中陆续填入。
