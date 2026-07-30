# Design: 惰性化跨包类型世界

## 背景：当前机制（全量 world）

`DepScan.ScanDirsLazy`（[DepScan.z42:242-342](../../../../src/compiler/z42c.pipeline/src/DepScan.z42)）：
1. Open 全部 ~34 个 zpkg（读字节 + 解码 STRS 池）。
2. **`TsigReconcile.BuildWorld`**（[TsigReconcile.z42:25-33](../../../../src/libraries/z42.ir/src/TsigReconcile.z42)）：
   对**每个包**调 `ReadModuleTypes` + `ReadModuleSigs`，解析完整 TYPE/SIGS → `Wp: ReconWorldPkg[]`。**这是主导成本，O(总量)。**
3. 从 NSPC 建 nsMap（廉价）。
4. 只对 prelude（z42.core）`Rebuild` 进 `Exported`；其余包 `Loaded=false`，靠 `EnsurePackageLoaded` 按需 Rebuild。

`TsigReconcile.Rebuild` → `_rebuildClass`（[:146-285](../../../../src/libraries/z42.ir/src/TsigReconcile.z42)）用 `Wp` 做两件事：
- **按 FQ 名定位类**：`_locate`（[:348-363](../../../../src/libraries/z42.ir/src/TsigReconcile.z42)）+ 基类链 walk（[:166-177](../../../../src/libraries/z42.ir/src/TsigReconcile.z42)）**扫遍整个 Wp** 找 `BaseName`（FQ）对应的类。
- **读祖先 SIGS**：`Wp[chainPkg[ci]].Sigs[chainMod[ci]]`（[:222](../../../../src/libraries/z42.ir/src/TsigReconcile.z42)）。

BuildWorld 全量，只为让「扫遍 Wp 找祖先」永远命中。

## Spike 实证（决策依据）

在真实 34 包世界（stdlib + z42c）跑「路由定位 vs 全量扫描定位」差分（`/tmp/spike/spike_routing.z42`）：

```
WpCount=34
checked=230  mismatch=0  routed_missed=0            # 230 条基类链，路由与全量扫描 100% 一致，0 漏
closure: max=3  avg=1  | closure<=2: 32/34  <=4: 34/34   # 加载一个包，基类链闭包最多牵连 3 个包
```

**结论**：① 基类链祖先可靠命名空间路由精确定位（等价全量扫描）；② 闭包极小且局部（不随库增大）。
→ 惰性化安全，且能实现 O(引用闭包)。

## Architecture

```
ScanDirsLazy:
  Open 全部包（STRS + NSPC）              # 残余 O(包数) 轻扫描（建 nsMap 必需，见 Deferred）
  建 nsMap（ns→包）+ nsIdxMap（ns→world 索引多重）   # 从 NSPC，廉价
  world = LazyReconWorld(worldZpkgs, dirs, nsIdxMap)  # Wp 全 null，按需填
  只 Rebuild prelude（world）             # 触发 world 填充 prelude + 其祖先闭包（≤几个包）

首次 eval "1+2"：不引用外部 → world 一个额外包都不填 → 近零
首次 eval "Console.WriteLine": 引用 Std.IO → EnsurePackageLoaded → Rebuild(z42.io, world)
  → world.EnsureFq(基类 FQ) 路由填充 z42.io + 其祖先（闭包 ≤3）→ 只解析这几个包
```

## Decisions

### Decision 1: `Wp` 改为懒填数组，封装 `LazyReconWorld`

`Wp: ReconWorldPkg[]` 条目初始 `null`，首次触碰才解析。封装为 `LazyReconWorld`，随身带路由所需的世界
信息：

```
public sealed class LazyReconWorld {
    ZpkgInfo[] World; string[] WorldDirs; int Wc;   // 供按需 ReadModuleTypes/Sigs
    ReconWorldPkg[] Wp;                              // 懒填：null = 未解析
    // ns → world 索引多重映射（从 NSPC 预建，路由用）
    string[] NsKeys; int[][] NsToIdx; int NsN;      // 或等价平行结构
    ReconWorldPkg EnsureIdx(int i);                 // 填 Wp[i]（幂等）
    void EnsureFq(string fq);                        // 路由 fq→ns→所有声明 ns 的包 → EnsureIdx 各个
}
```

`EnsureIdx(i)`：`if (Wp[i]==null) Wp[i] = new ReconWorldPkg(ReadModuleTypes(World[i],WorldDirs[i]), ReadModuleSigs(World[i]));`

### Decision 2: 路由 = FQ→ns→「所有声明该 ns 的包」（非 first-wins）

`EnsureFq(fq)`：`ns = _nsOf(fq)`（最后一个 `.` 之前——嵌套 `+`/泛型 `$` 不含 `.`，spike 证 230/230
切分正确）；查 `nsIdxMap[ns]` 得**所有**声明该 ns 的 world 索引；对每个 `EnsureIdx`。

**为何「所有」而非 first-wins**：一个 ns 可能横跨多包（A、B 都 `namespace Std.Foo`，而 `Std.Foo.Bar`
只在 B）。沿用现 `EnsurePackageLoaded` 的「加载所有声明该 ns 的包」语义，保证与全量扫描等价。
spike 的 `_routedLocate`（扫所有 ns 匹配模块）即此语义，实测 0 漏。`nsIdxMap` 从各包 NSPC 预建
（O(包数) 一次，廉价；取代按需重扫 NSPC）。

### Decision 3: `_locate` / 基类链 walk 改「路由 + EnsureFq + 跳 null」

- 定位 FQ `bn` 前先 `world.EnsureFq(bn)`（把 bn 所在包填进 Wp）。
- 扫描循环 `while (p < wc)` 跳过 `Wp[p]==null` 的条目（未填充的包不参与匹配——它们与 bn 无关，
  否则早被 EnsureFq 填了）。
- 逻辑其余不变 → 找到相同的 (pkg, mod)、相同顺序（spike 保证）。

`_rebuildClass` 开头 `_locate(cd.Name)` 定位自身：先 `EnsureFq(cd.Name)`（其实 = 目标包，Rebuild 已填）。

### Decision 4: `Rebuild` 入参 `wp[]`→`LazyReconWorld`；复用目标包已解析条目

`Rebuild(z, zDir, world)`：先 `world.EnsureFqPkg(z)`（把目标包 z 填进 Wp[zIdx]），**复用该条目的
Types/Sigs**（消除当前 [:37-38](../../../../src/libraries/z42.ir/src/TsigReconcile.z42) 的重复解析）。
调用点（DepScan 的 prelude Rebuild / `_loadOpenedPackage` / `ExtendWithPackage`）随之改传 `world`。

### Decision 5: 正确性 = 路由等价（spike 证）+ 解析确定 → 逐字节不变

- 路由找到与全量扫描**相同**的祖先（spike 230/230）。
- 每个包的 `ReadModuleTypes/Sigs` 解析是**确定性**的 → 懒填与 eager 填出的 `ReconWorldPkg` 逐字段相同。
- 基类链 walk 逻辑不变（只加「先 Ensure、跳 null」）→ 相同祖先、相同 topmost-first 合并顺序。
- `Exported`/`DepIndex` 的填充**顺序**不变：今天就是 prelude eager + 其余按引用序懒加载（本 change
  只把 TYPE/SIGS 的**解析**也变懒，不改加载/索引顺序）。
- → 产物**逐字节不变**，自举 gen1==gen2 为最终铁证。

### Decision 6: 残余 eager `Open`（STRS）—— 本 change 不动，记 Deferred

去掉 BuildWorld 后，`ScanDirsLazy` 仍 eager `Open` 全部包（读字节 + STRS 池解码）以建 nsMap。这是更轻的
O(包数) 头部扫描（非 O(总量元数据)）。进一步降到 O(引用) 需延后 Open/STRS 或加全局 ns 清单——见 Deferred。
本 change 的目标是**干掉主导的 BuildWorld**，把陡峭的 O(总量) 降成平缓的 O(包数)。

## Implementation Notes

- `nsIdxMap`（ns→world 索引多重）：在 ScanDirsLazy 收集 world 的循环里，读每包 NSPC 建（已在读 NSPC 建
  nsMap，顺带建多重版）。
- `guard < 32` 链深上限不变。
- 接口（`_rebuildInterface`）**不走**基类链路由（接口作名字存 `InterfaceCount`，不 `_locate`）——spike 只测
  基类链（单继承），接口不在闭包内。`class X : IFace@其他包` 存 IFace 裸名，不触发 world 填充。保持现状。
- `ExtendWithPackage`（REPL 增量并入 Repl.R{N}）：向 `LazyReconWorld` 追加一个 world 条目 + 更新 nsIdxMap；
  其 Wp 条目按引用懒填。

## Testing Strategy

- **自举字节不动点 gen1==gen2**（编译器自身大量跨包基类链，最强回归网——**最终铁证**）。
- **cross-zpkg 全套**（基类链跨包：`subclass_catch` / `vcall_base_fallback` / `impl_propagation` /
  `interface_impl` / `generic_field_carry` 等）。
- **stdlib 全绿**（`xtask test stdlib`，23 lib）+ **e2e goldens**。
- **开发期差分断言**（临时挂、验完撤）：对每个 stdlib 包，`Rebuild` 用 eager-Wp vs `LazyReconWorld` 产出
  的 `ExportedModuleZ[]` **逐字段比对**——直接抓「懒填漏祖先/顺序错」。
- **REPL 端到端**：eval 正确性 + 首次-eval 延迟实测（`1+2` 近零外部、`Console.WriteLine` 只付 Std.IO 闭包）+
  「库变多」外推（闭包不随包数增长）。

## Deferred / Future Work

### lazy-type-world-future-defer-open-strs
- **触发原因**：去掉 BuildWorld 后，残余 eager `Open`（STRS 解码 ×包数）是 O(包数) 轻扫描（建 nsMap 必需）。
- **前置**：延后 STRS 全解码（只读 NSPC/META 路由）或加**全局 ns→包清单**（免逐包 Open）。
- **触发条件**：包数增长到残余 Open 成为体感瓶颈时。

### lazy-type-world-future-symbol-name-index
- **来源**：REPL 补全「立即列出全部 stdlib 符号」的需求（用户三层架构第 1 层）。
- **触发原因**：本 change 后补全覆盖面 = 已加载包（同今天）；要立即全量需后台名字索引。
- **前置**：轻量符号名扫描（或 SYMS 格式段，格式 bump 两-nightly）。与「不预加载默认 using」一起做，避免补全回退。
