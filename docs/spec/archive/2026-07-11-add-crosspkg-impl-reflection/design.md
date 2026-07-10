# Design: 跨包 impl 反射（P1-e ①）

## Architecture

```
zpkg bytes ──read_zpkg_impl_pairs──► Vec<(target_fq, trait_fq)>   ← 跳过 type_args + 方法
     │（packed + indexed 都有 IMPL 顶层段）
     ▼
LoadedArtifact.impl_pairs ──lazy_loader 合并──► LazyLoader.impls: HashMap<target_fq, Vec<trait_fq>>
     │（主模块经 seed_impls 并入同一注册表）
     ▼
builtin_type_interfaces：base 链每类 queue += impls[类FQ] → 既有 BFS 传递闭包（trait 基接口自动展开）
```

## Decisions

### D1: 独立轻量解析（read_zpkg_impl_pairs），不 reshape Module
IMPL 是 zpkg 顶层段（与 MODS 平级），索引 zpkg 共享 STRS 池。独立函数从 raw bytes 读
dir+STRS+IMPL，返回 pairs——不动 `read_zpkg_modules` 返回类型（零 ripple）。STRS 重复解码一次
（每 zpkg 加载一次，代价可忽略）。packed / indexed 同一函数（两者段面都含 IMPL）。

### D2: 只取 (target, trait)，方法记录跳过
反射 GetInterfaces 只需 trait 关联；impl 方法派发已有机制（vtable + func_index）。方法记录
定长可跳：`name(4)+ret(4)+vis(4)+flags(1)+min_arg(2)+param_count(1)+params_from(1)=17B + pc×8`
（镜像 `_writeMethod`）。type_args：`u8 count + count×u32` 跳过。

### D3: 注册表挂 LazyLoader，主模块 seed 并入
跨包聚合天然归 lazy loader（type_registry 同款模式）。`load_zpkg_file` 合并 artifact.impl_pairs
（多包对同一 target 的 impl **追加**，不 first-wins——不同包各自 impl 不同 trait 合法）。主模块
zpkg 的 impls 经 `seed_impls`（镜像 `seed_types_for_lookup`）并入。VmContext 加 `impl_traits_for`
转发（reflection builtin 用）。

### D4: 反射只见已加载包（语义）
包 B 未加载 → 其 impl 方法不可调 → GetInterfaces 不含其 trait，一致。无兜底扫描（不为反射
强制加载全部 declared zpkg——按需加载哲学不变）。

### D5: 无格式 bump
纯读现有段。无两代自举、无 fixture regen、无 golden 变化。z42c 零改动。

## Testing Strategy
- 单测：人造 IMPL 字节 → read_zpkg_impl_pairs 解析（含方法跳过对齐）。
- e2e cross-zpkg：新 fixture `impl_reflect`——包 B `impl Greeter for A.Thing`，主程序
  `typeof(Thing).GetInterfaces()` 含 "Greeter"（+ 派发不回归）。
- 全 GREEN + cargo。
