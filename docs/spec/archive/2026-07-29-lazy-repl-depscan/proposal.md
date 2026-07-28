# Proposal: REPL 依赖世界惰性扫描——首轮 4.3s → ~0.3s

> 状态：🔴 DRAFT（compiler 行为变更，需规范先行；待 User 6.5 裁决 D1–D3 再 IMPL）
> 创建：2026-07-29 | 拟占子系统：`compiler`（`z42c.pipeline` DepScan）+ `stdlib`（`z42.ir` TsigReconcile 闭包）+ `toolchain`（Script.Create 接线）

## Why（含实测）

REPL 首次 eval 花 **~4.3s** 建全量依赖世界（`DepScan.ScanDirs` 扫 ~30 个 stdlib 包做 `TsigReconcile.BuildWorld` + 逐包 `Rebuild`）。**实测**（interp，同 REPL 路径）：

| | 耗时 | 模块数 |
|---|---|---|
| 全量 scan | **4343ms** | 374 |
| 只扫 z42.core（prelude） | **306ms** | 72 |

即：绝大多数首轮只需要 prelude，却为 29 个没用到的包付了 ~4s。Python 之所以启动快，正是**惰性 import——不预建整个世界**。本提案把 REPL 依赖世界从"全量预扫"改为"**只扫 prelude + 按需加载引用到的包**"，天花板 **~93% 削减**（4343→306ms），之后首次引用某包 ~140ms/包、用完即缓存——对齐 Python 的 per-import 模型。

## 关键机制（调研实据，file:line）

**成本几乎全在 TsigReconcile**（`z42c.pipeline/DepScan.z42`）：
- `TsigReconcile.BuildWorld`（`ScanDirs` L97）——全 world TYPE/SIGS 一次性解析；
- 逐包 `TsigReconcile.Rebuild`（L135）——每类在全 world 上走 base 链，O(deps×world×classes)，**4.3s 主体**。
- 廉价部分：NSPC 命名空间索引（L106-122，`ZpkgReader.ReadNamespaces` 只读一个段）+ `DependencyIndex.AddModule/Seal`（扁平 StrMap，无跨包依赖）。

**现成可复用原语**：
- **`DepScan.ExtendWithPackage`**（`DepScan.z42:165`）——已是"增量并入单包（NSPC+DepIndex+world+Exported 四件）"原语，REPL 每轮用它并入自己的 `Repl.R{N}` 包（`Script.z42:131/183`）。它做 `TsigReconcile.Rebuild` 复用 `scan.Wp`。**限制**（作者注释 L163/196-198）：假设"新包无跨包 base 链、静态 world 足够"——用于任意 stdlib 包时**缺 base 链闭包**（见难点）。
- **`DepScanResult.FileOf(ns)` + `NsNames/NsFiles`**（`DepScan.z42:32`）——ns→zpkg 定位，惰性触发的路由表。
- **`ZpkgReader.Open`+`ReadNamespaces`**——只读 NSPC 的轻量扫描（不解 TYPE/SIGS），建全量 nsMap 极廉价。
- **VM 侧 `lazy_loader.rs`**——已实现完整的"启动只加载 z42.core、ns 路由、miss 按需加载、**transitive dep 闭包 + 环安全**"（`load_zpkg_file`/`force_load_all_declared` L301/331）。**其闭包展开算法可直接照搬为编译期 base 链闭包**。

**编译期触发点**（`PackageCompile.Compile` L73-77 复用 CachedScan；`ImportedSymbolLoader.Load` L103 按 using 激活）：引用未加载包的符号 → 类型检查报 **E0401**（`ExprTyper.z42:163` `undefined:`／`MemberResolver.z42:48` `no method`），但错误**只带短名、不带 namespace**——短名→包无索引。故**`using ns` 是更稳的触发**（namespace 显式已知 → `FileOf(ns)` 直接定位包）。

## 提议的设计

1. **首轮：prelude world + 全量廉价 nsMap**（替换 `Script.z42:68` 的全量 `ScanDirs`）
   - DepScan 新增入口 `ScanDirsLazy`：**只对 prelude 包（z42.core）做 BuildWorld/Rebuild**（~306ms）；对**全部包只 `Open`+`ReadNamespaces` 填 `NsNames/NsFiles`**（廉价，不 BuildWorld）。nsMap 必须全量（否则触发时无法 `FileOf(ns)` 定位）。
2. **按需并入原语**：扩展 `ExtendWithPackage`（或新增 `EnsurePackageLoaded(scan, ns)`）
   - 从磁盘 `File.ReadAllBytes(FileOf(ns))` 读 stdlib 包 → 并入；传**真实 pkgDir**（现传 `""`，对 indexed 包 TYPE 为空，`ZpkgReader.z42:240-266`）；
   - **递归展开该包 DEPS + base 链祖先包闭包**并一并入 world（照搬 VM `lazy_loader` 的 transitive 闭包 + 环安全）——**这是正确性关键**（见难点）。
3. **触发接线**（D1 决定策略）：REPL 每轮聚合 usings 后，对每个"包未加载"的 `using ns` → `EnsurePackageLoaded(scan, ns)`。默认 using（#64 的 Std.IO/Collections/Text/Math）按同一路径处理（D2 决定 eager/lazy）。
4. **`ImportedSymbolLoader` 不改逻辑**（`ImportedSymbolLoader.z42:38` 已用 `active[]` 门控）——`scan.Exported` 惰性变小后，其每轮 ~50ms 自然下降。

## 待 User 裁决（6.5 gate）

- **D1｜触发策略**：
  - **(A, 推荐)** **load-on-using**——处理 `using ns` 时若包未加载即并入。namespace 显式已知、稳、无需短名→包表。缺点：`using` 了但没用到的包也会加载。
  - (B) **load-on-reference**——引用符号时才加载（Python 式，最省）。但 E0401 只有短名 → 需建"短名→包"表或捕获 E0401 后遍历未加载包重试（fallback，VM 侧亦保留此 fallback B）。复杂度高。
- **D2｜默认 using 的包何时加载**：
  - **(A, 推荐)** **首用即加载**：默认 using 的 4 包在**首次真正引用**其符号的那轮才并入（保持"只 `1+2`"的会话仍 ~306ms）。
  - (B) 启动即加载 4 默认包（简单，但启动 ~306ms+4 包 ≈ ~1s，牺牲纯 306ms）。
  - 注：D2 与 D1 交互——若 D1=A（load-on-using），默认 using 在 Create 时即"声明"，需决定是否推迟到首用。
- **D3｜base 链闭包的完整性保证**：并入包 X 时，必须传递闭包地并入 X 的 DEPS + base 链祖先所在包，否则 `TsigReconcile._rebuildClass`（`TsigReconcile.z42:146-180`）**静默丢继承成员**（`_locate` 返回 -1、`ancSigs=null`，无诊断）。
  - **(A, 推荐)** 照搬 VM `lazy_loader` 的 transitive-dep 闭包（读 DEPS 段递归 + 环安全 pre-insert），并入包时展开到稳定。
  - 需确认：新祖先加入 world 后，已并入包**不回填重扫**（`ExtendWithPackage` 只 Rebuild 新包）——要么保证加载顺序（先祖先后子），要么补一次针对受影响包的重扫。

## Scope（初估，D1=A/D2=A/D3=A 下）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.pipeline/src/DepScan.z42` | MODIFY | `ScanDirsLazy`（prelude world + 全量 nsMap）；`EnsurePackageLoaded(scan, ns)`（磁盘读 + 真实 pkgDir + DEPS/base 闭包展开） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY? | 若需"新增祖先后回填重扫受影响包"，补一个针对性重扫入口 |
| `src/libraries/z42.ir/src/ZpkgReader.z42` | 复用 | `Open`/`ReadNamespaces`/`ReadModuleTypes(pkgDir)`/DEPS 读取 |
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `Create` 首轮改 `ScanDirsLazy`；每轮 using 处理接 `EnsurePackageLoaded` |
| 测试 | NEW | 惰性正确性（跨包 base 链继承成员不丢）+ 性能（首轮 core-only ~300ms；首次引用某包后其类型/成员可用）；对比全量 scan 结果一致 |
| `docs/design/toolchain/repl.md` + `docs/design/compiler/` | MODIFY | 惰性扫描机制页；`repl-future-persist-static-scan` 邻域更新（惰性 ≠ 持久化，两者可叠加） |

## 子系统 / 锁

`compiler`（主）+ `stdlib`（z42.ir）+ `toolchain`。**compiler 现被 `unify-run-modes` 占**（ACTIVE.md）→ IMPL 前按互斥锁排队，DRAFT 不占锁。

## 非目标

- 持久化落盘（`repl-future-persist-static-scan`，方案 B）——与惰性正交、可叠加，本提案不做。
- 异步预热（方案 A）——测量后收益边际（D 落地后首轮 ~306ms 无需再藏），暂挂。
- 非 REPL 路径的 DepScan（`build`/`test` 全量编译）——不变，惰性仅用于 REPL 交互路径。

## 风险

- **base 链闭包不全 → 静默丢继承成员**（最大风险，D3）。缓解：闭包展开照搬 VM 已验算法 + 新增"惰性 vs 全量结果一致性"测试（同一输入两条路径产物比对）。
- 首次引用某包的 ~140ms 卡顿（可接受，Python 同量级；若烦扰再叠加方案 A 异步预热）。
- indexed 包需真实 pkgDir（现 `ExtendWithPackage` 传 `""`）——务必修正。
