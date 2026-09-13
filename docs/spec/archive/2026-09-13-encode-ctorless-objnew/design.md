# Design: `ObjNew.CtorKnown` —— 零构造器的可区分编码

> 语义与判据的长期 SoT 已上浮到
> [`docs/book/src/runtime/missing-symbol-resolution.md`](../../../book/src/runtime/missing-symbol-resolution.md)
> 「构造器 —— `missing_ctor_exception`，判据是编译期的**正向位**」。本文件只留**决策与取舍**。

## D1 — 为什么必须加 wire 位（没有第三条路）

需要表达**三**种状态：① 编译期确实看见了这个构造器 ② 这个类确实零构造器 ③ 证不出来。
而 ctor 名字段只能表达两态：原名 / 改名。且：

- 改「已证实」那一侧的名字 ⇒ 直接破坏运行期解析。
- 改「零构造器」那一侧（发空名，复用 `IrLoopAllocReuse` 的裸分配约定）⇒ 把 ② 与 ③ 合并，
  于是**新引入**一种静默：编译时 v2 无构造器 ⇒ 发空名；运行时 v1 有构造器 ⇒ 被悄悄跳过。

⇒ 加位。zbc 1.39 / zpkg 0.44，尾部一个 u8，与 1.29 的 `stack_alloc` 同款位置与写法。

## D2 — 为什么是**正向**（`CtorKnown`）而不是反向（`Ctorless`）

反向位的缺席态是「不是零构造器」⇒ 1.38 及更早的全部产物会被判成「构造器缺失」⇒ 全炸。
正向位的缺席态是「证不出来」⇒ 旧产物、裸分配、单文件 `--emit-zbc` 一律退回既有行为。
**保守态必须是位为 0**，这决定了位的方向。

## D3 — 落点：`PackageCompile` ⑩ 装配点，不是发射端

发射端（`CallEmitter._emitNew`）那一刻答案不存在，两个独立原因（合成构造器晚于绑定产生 /
本 CU 看不到同包其它文件），见 book 页。装配点是本包全部 `IrModule` 第一次同时在手的时刻，
`DependencyIndex` 也在手。

**不做成 per-module IR pass**（与 `IrEscapeAnalysis` / `IrLoopAllocReuse` 不同族）：那些是
函数内分析，这条要跨 CU。

**增量**：每次装配**重算**（覆盖写，非 OR），cached `IrModule` 一并重扫 ⇒ 不留过期结论。

## D4 — oracle 用完整 FQ，不用短类名键

`DependencyIndex.Statics` 同时注册了短类名键（`Cls.Method`，first-wins）与完整 FQ 键
（`AddModule` 的 `Statics.TryAdd(name, entry)`）。`CtorKnownFixup._visible` **只查完整 FQ**
—— 短键是模糊键，会把另一个包的同短名类误判成命中，从而给一个其实不存在的构造器置位。

## D5 — runtime 判据取并集，不替换

```rust
ctor_missing_is_definite(name, argc, known) = !name.is_empty() && (known || argc > 0)
```

`argc > 0` 那条（`fix-silent-symbol-resolution` 站点 ③ 的原判据）独立成立、继续保留：
零构造器的类不可能接受实参。**只增覆盖、不减**。判据抽成无 `VmContext` 的纯函数，
便于穷举单测（`symres_tests.rs` 四条）。

## D6 — 格式 bump 的本地验证路径

本地 `cargo` 建出的是新格式 VM，而种子 z42c/stdlib 是旧格式 ⇒ warm 建直接墙掉
（实测：`zpkg minor 43 not supported (writer is at 0.44)`）。**不走**两代自举（macOS 本地有
独立环境墙），走 [`version-bumping.md`](../../../../.claude/rules/version-bumping.md)
「本地全量验证 / fixture 重生的配方」：先推 PR → CI `compile-toolchain` 用当前源码建出新格式
工具链并 `upload-artifact` → 下载 overlay 成本地种子 → 种子与 cargo VM 同格式 ⇒ warm 建 / 测 /
fixture 重生全通。

## D7 — 门

本仓自身几乎不产生阳性证据（普查：3899 个 `ObjNew` 站点里 imported 侧只有 5 个确实零构造器），
故必须自带 fixture。`src/tests/cross-zpkg/`：

| fixture | 证明什么 |
|---------|---------|
| `ctorless_objnew_skew` | 阳性：v2 有 `Gadget()`、v1 **零构造器** ⇒ `new Gadget()` 抛 `MissingSymbolException`。**旧编译器上必须 FAIL**（打印 `constructed 0`）—— 退回对照 |
| `ctorless_objnew_present` | 门有判别力：不 skew 时照常构造，打印 `constructed 7`（防「恒抛」） |
| `ctorless_objnew_absent` | **过度收紧守卫**：真·零构造器跨包类 `new Holder()` 不得误报 |

加上 `symres_tests.rs` 四条纯判据单测（含「未置位 + 零实参必须放行」这条不变式）。
两后端都要跑（`xtask test e2e --dir cross-zpkg` + `--mode jit`）。
