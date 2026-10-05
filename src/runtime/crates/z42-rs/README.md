# z42-rs

## 职责

z42 Tier 2 native interop API（用户面向）。Rust 库作者通过实现这里的 trait（手写或借助 `z42-macros` 的 derive）把自己的类型暴露给 z42 用户代码。

需要 `std`（`z42-abi` 才是 `no_std`）；依赖 `z42-abi` + `z42-macros`，不依赖 runtime crate。

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/lib.rs` | crate 入口；`prelude` 模块 + `z42_abi` 重导出 |
| `src/native_helpers.rs` | 宏生成代码用的运行期辅助（panic → 错误码 Z0905 等） |
| `src/types.rs` | `Z42Args` / `Z42Value` / `Z42TypeRef` / `Z42Error` / `Descriptor` 用户友好别名 |
| `src/traits.rs` | `Z42Type` / `Z42Traceable` / `Visitor` trait 骨架 |
| `tests/skeleton_tests.rs` | 验证用户能手写实现这些 trait（不依赖 macro） |

## 入口点

`use z42_rs::prelude::*;`：

| 名称 | 意图 |
|------|------|
| `Z42Type` | 用户类型必须实现 |
| `Z42Traceable` | 类型持有其他 GC 引用时实现（参与循环检测） |
| `Visitor` | trace 回调 |
| `Z42Args` / `Z42Value` / `Z42TypeRef` / `Z42Error` | 跨界数据类型 |
| `Descriptor` | `Z42TypeDescriptor_v1` 的别名 |

## 待办

`#[derive(Z42Type)]` / `#[trait_impl]` / reverse-call 等高层能力待 source generator（C5）联动设计；主入口是 `#[z42::methods]` + `module!`，`Z42Type` trait 由宏自动 emit。

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
```

## 依赖关系

- 上：`z42-macros`（derive 实现）、用户 native 库
- 下：`z42-abi`（ABI 类型镜像）

## 如何测试验证

```bash
(cd src/runtime && cargo test -p z42-rs)    # 手写实现 trait 的骨架测试
```
