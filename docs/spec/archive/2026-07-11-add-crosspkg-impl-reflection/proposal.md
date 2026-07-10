# Proposal: 跨包 impl 反射（unify-type-metadata P1-e ①）

## Why

`impl TraitT for TypeA` 声明在包 B、TypeA 在包 A 时，运行时**派发正常**（vcall 走 vtable +
func_index 按名解析），但 **VM 不读 IMPL 段** → `typeof(TypeA).GetInterfaces()` 反射不到
B 加的 TraitT——反射缺口（initiative D2 已查证）。unify-type-metadata 把 IMPL 重定性为
「统一元数据」：z42c + VM 都读。P1-e 第一半。

## What Changes

- VM 读 zpkg **现有 IMPL 段**（无格式 bump）：解析 `(target_fq, trait_fq)` 对（跳过 type_args
  + 方法签名——方法派发已有机制，反射不需要）。
- `LoadedArtifact.impl_pairs` + `LazyLoader.impls` 全局注册表（target_fq → [trait_fq]，随
  zpkg 加载聚合；主模块 zpkg 经 seed 并入）。
- `builtin_type_interfaces`（GetInterfaces/GetInterface 共用）：沿 base 链每类额外并入注册表
  中该类的 impl traits，进入既有 BFS 传递闭包（含 trait 的基接口）。
- 加载语义：反射只见**已加载**包的 impl（B 未加载则其 impl 方法本也不可调，一致）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | `read_zpkg_impl_pairs(raw)`（dir+STRS+IMPL 独立解析；方法记录 17+pc×8 跳过器） |
| `src/runtime/src/metadata/loader.rs` | MODIFY | `LoadedArtifact.impl_pairs`；packed/indexed 两路填充 |
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | `impls` 注册表 + 合并 + `impl_traits_for` + `seed_impls` |
| `src/runtime/src/vm_context.rs` | MODIFY | `impl_traits_for` 转发 |
| `src/runtime/src/main.rs` | MODIFY | 主模块 artifact impl_pairs seed 进 lazy loader |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `builtin_type_interfaces` 并入 impl traits |
| `src/tests/cross-zpkg/impl_reflect/*` | NEW | e2e：跨包 impl 后 GetInterfaces 含 trait |
| `docs/design/language/reflection.md` | MODIFY | 跨包 impl 反射节 |
| `docs/design/runtime/zpkg.md` | MODIFY | IMPL 段「消费者」补 VM（无格式变化） |
| `src/runtime/src/metadata/loader_tests.rs` 或 `lazy_loader_tests.rs` | MODIFY | impl_pairs 解析单测（如需） |

**只读引用**：`src/compiler/z42c.project/src/ZpkgWriter.z42`（_buildImpl/_writeMethod 布局镜像）、
`src/tests/cross-zpkg/impl_propagation/`（既有 fixture 参考）。

## Out of Scope

- `is`/`as`/`IsAssignableFrom` 的跨包 trait 判定（派发已工作；本砖只补反射面）
- delegate 元数据（P1-e ②，独立 change）；删 TSIG/EXPT（P3）

## Open Questions
（无——机制已查证，纯 additive 读取）
