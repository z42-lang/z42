# Tasks: interp 符号表 SipHash → FxHash

> 状态：🟢 已完成 | 完成：2026-08-17 | 类型：refactor（纯内部，无行为/字节变更）

**变更说明：** 把 VM 内部符号解析用的 `HashMap<String, _>`（std 默认 SipHash）换成
`rustc_hash::FxHashMap`（FxHash）。

**原因：** z42c 自编译 profile：~14.7% self-time 在符号解析 hashmap（`sip::write` +
`hash_one` + `memcmp` + `try_lookup_*`），主要由 megamorphic VCall IC miss 每次 hash 一个 FQ
名字符串驱动。这些表都是 VM 内部（非 hash-flooding 面），换 FxHash 在短字符串键上快 ~5–10×。

**文档影响：** 无需 book/README 改动（纯内部实现，不改任何外部行为、机制、命令面、格式）。
`interp/README.md` 若有「HashMap 符号表」描述则顺带一句提及 hasher；否则跳过。

**杠杆归属：** interp 大杠杆优化程序 · 杠杆 1（见 memory `post-layout-jit-perf-program` Tier 3 /
新起的 interp 子系统线）。

## 目标 map（仅符号解析热点，不碰无关同型 map）
- [x] 1.1 `Module.type_registry` / `func_index`（bytecode.rs，`#[serde(skip)]` → 零 wire 影响）
- [x] 1.2 loader.rs `build_type_registry` 系列构建器/签名（产出/接收 FxHashMap）
- [x] 1.3 `LazyLoader.function_table` / `type_registry` / `declared_zpkgs` / `impls`（lazy_loader.rs）
- [x] 1.4 `seed_types_for_lookup` / `seed_lazy_loader_types` 签名（&FxHashMap）
- [x] 1.5 `VmContext.static_field_index` / `interned_cache`（vm_context.rs）
- [~] 1.6 `Function.block_index`（分支目标冷回退路径）— **本 PR 剔除**：热路径走已预解析的 `branch_targets`，block_map 仅 hand-built test fn 回退，近零价值且在被序列化的 `Function` 上 → 保持聚焦

## 验证
- [x] 2.1 cargo build --release --bin z42vm 无错
- [x] 2.2 cargo test --release --lib 全绿
- [x] 2.3 A/B 宏基准：big.z42 前端 typecheck，FxHash vs SipHash（基线 8.141s）
- [x] 2.4 xtask test all --skip zzz 全绿（e2e interp + vm-jit-consistency + stdlib + 自举 5/5 逐字节）

## 备注
- 无格式 bump、自举字节不动（VM 内部 hasher 与 zbc/zpkg emit 无关）。
- 确定性：FxHash 迭代序确定，比 SipHash 随机种子更符合 common-pitfalls §1；现有
  `.keys()`/`.values()` 迭代要么进 HashSet 要么 `.any()`，不依赖顺序。

## 结果
- A/B: 8.383s→7.425s = **1.13× (11.4% faster)**（前端 typecheck big.z42，各 6 runs）。
- emit-zbc 逐字节 identical（侧证无迭代序依赖）。
- GREEN: cargo--lib 917+21/0 + e2e interp + cross-zpkg + stdlib 280/22 + z42c[Test]24 + **自举 5/5 gen1==gen2 逐字节** + vscode-syntax。
- ⚠️cargo fix 越界：误删 lazy_loader 模块文档(恢复为 //!)+translate mut/host_tests Arc(git checkout 还原)。
