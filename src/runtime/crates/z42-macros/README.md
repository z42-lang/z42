# z42-macros

## 职责

z42 Tier 2 ergonomic Rust API 的 proc macro 实现。让 Rust 用户不必手写 `Z42TypeDescriptor_v1` 字面量即可把任意 Rust 类型暴露给 z42。

## 入口

| Macro | 形式 | 作用 |
|-------|------|------|
| `z42::methods` | `#[methods(module = "...", name = "...")]` 作用于 `impl T { ... }` | **C3 主入口**：一次性 emit descriptor + 方法表 + 所有 `extern "C"` shim + `Z42Type` impl |
| `z42::module` | `module! { name: "...", types: [T1, T2, ...] }` | 生成 `#[no_mangle] pub extern "C" fn <name>_register()` 把每个类型推入 VM |
| `z42::Z42Type` | `#[derive(Z42Type)]` | **C3 stub**：报清晰 `compile_error!` 指向 spec C5 |
| `z42::trait_impl` | `#[trait_impl("trait_name")]` | **C3 stub**：报清晰 `compile_error!` 指向 spec C5 |

## 用法示例

```rust
use z42_rs::prelude::*;
use z42_rs as z42;

#[derive(Default)]
pub struct Counter { value: i64 }

#[z42::methods(module = "demo", name = "Counter")]
impl Counter {
    pub fn inc(&mut self) -> i64 { self.value += 1; self.value }
    pub fn get(&self) -> i64 { self.value }
}

z42::module! {
    name: "demo",
    types: [Counter],
}

// Linker emits `demo_register` symbol; VM 启动时调它把 Counter 注册到
// `VmContext.native_types`。
```

## 子模块

| 文件 | 职责 |
|------|------|
| `src/lib.rs` | `#[proc_macro*]` 入口；路由到子模块 |
| `src/methods_attr.rs` | `#[z42::methods]` 实现（核心：解析 attribute + impl 块 → emit descriptor / 方法表 / shim / Z42Type impl）|
| `src/module_macro.rs` | `module!` 实现（生成 `<name>_register()`）|
| `src/signature.rs` | Rust 类型 → ABI 签名字符串映射；C3 仅 blittable 子集（i*/u*/f*/bool/raw ptr/SelfRef/void）|
| `src/shim.rs` | 每个用户方法 → `extern "C"` shim（`catch_unwind` 包裹，panic 经 `z42_rs::native_helpers::set_panic` 转 Z0905）|
| `src/util.rs` | c-string literal 生成、私有 ident 命名、模块名校验 |

## 状态

`#[derive(Z42Type)]` 与 `#[trait_impl]` 尚未实现，待 source generator (C5) 一并设计 z42-side trait 形状。

## 依赖关系

- 上：`z42-rs`（重导出 macros 给最终用户）
- 下：`proc-macro2`、`quote`、`syn`、`convert_case`
- 平级：`z42-abi`（macro 展开引用其类型 / 常量；不直接依赖）

## 测试

- `tests/expand_smoke.rs` + trybuild：4 个诊断 case（缺 module/name attr、非法 module 名、derive 重定向消息）
- 端到端（`z42_vm` 测试 binary 内）：`tests/native_interop_e2e.rs::rust_counter_*` 验证宏生成的 descriptor 与 C 写的等价
