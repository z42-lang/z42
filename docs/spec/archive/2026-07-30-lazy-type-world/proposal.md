# Proposal: 惰性化跨包类型世界（DepScan / TsigReconcile）

## Why

REPL 首次 eval（无打字间隙时立即输入)的主导成本是 `TsigReconcile.BuildWorld`——它在
`DepScan.ScanDirsLazy` 里**一次性把全部 ~34 个 stdlib+编译器包的完整 TYPE/SIGS 元数据**
（每个类的字段/类型参数/约束/枚举成员 + 每个函数的完整签名）解析一遍，**无论输入引用了什么**。

这是 **O(标准库总量)** 的成本：标准库越大，每次 REPL 首次 eval 越慢——线性恶化。而它对编译一句
代码**大部分是浪费**：`1+2` 什么外部包都不碰；`Console.WriteLine` 只碰 `Std.IO`。

BuildWorld 之所以全量，只为保证「重建类的基类链时，导入的祖先类总能在 world 里找到」。但 spike
实测（见 design）证明：**基类链祖先可以靠命名空间路由按需定位，无需预解析全世界**。

## What Changes

把 `BuildWorld` 的**一次性全量解析**改成**按包懒填**：

- `ScanDirsLazy` 不再 eager 调 `BuildWorld`；`Wp`（world 的 TYPE/SIGS 解析缓存）改为**空数组 +
  按需填充**。
- `TsigReconcile.Rebuild` 重建某个类的基类链时，遇到导入的祖先 FQ → **取其命名空间 → 路由到声明该
  命名空间的包 → 只解析那些包进 `Wp`**（递归覆盖传递闭包）。
- 结果：加载一个包只解析它 + 其基类链祖先包（spike 实测**闭包 max=3、avg=1**），而非 34 个。
  **首次 eval 成本 = O(引用闭包)，不随标准库增大而恶化。**
- 顺带：`Rebuild` 复用已解析的 `Wp[目标包]`，消除当前「BuildWorld 已解析、Rebuild 又重解析一遍」
  的双重解析。

**不改**：zbc/zpkg 二进制格式（零 bump）、VM、语法、补全覆盖面（`Exported` 的填充口径不变——仍
prelude eager + 其余按需，见 Out of Scope）。产物**逐字节不变**（自举不动点为证）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | 新增 `LazyReconWorld`（懒填 Wp + ns→包索引路由 + `EnsureFq`）；`Rebuild` 入参 `wp[]`→`LazyReconWorld`；`_rebuildClass` 基类链 walk + `_locate` 改为「路由 + EnsureFq + 跳过未填充条目」；删 `BuildWorld` 的 eager 全量语义（保留为懒填单包的内部 helper） |
| `src/compiler/z42c.pipeline/src/DepScan.z42` | MODIFY | `ScanDirsLazy` 不再 eager `BuildWorld`；主循环从 NSPC 建 `ns→world 索引`多重映射；`DepScanResult` 携 `LazyReconWorld` 取代 `Wp[]`；`_loadOpenedPackage`/`EnsurePackageLoaded`/`ExtendWithPackage` 改用 `LazyReconWorld` |
| `docs/design/toolchain/repl.md` | MODIFY | 「启动预热」节补惰性类型世界机制（首次 eval O(引用闭包)）；更新 Deferred |
| `src/libraries/z42.ir/README.md` | MODIFY | 功能索引登记 `LazyReconWorld` |
| `src/compiler/z42c.pipeline/README.md` | MODIFY | 功能索引更新（DepScan 惰性世界） |

**只读引用**：
- `src/libraries/z42.ir/src/ZpkgReader.z42` — `ReadModuleTypes`/`ReadModuleSigs`/`ReadNamespaces`（懒填单包）
- `src/toolchain/scripting/src/Script.z42` — REPL 消费侧（Prewarm/_ensureWarm，理解调用链，本 change 不改）
- `src/tests/cross-zpkg/` — 基类链跨包用例（验证参照）

## Out of Scope（另立 follow-up）

- **不预加载默认 using（原 B）**：会让补全里 `Console/List/StringBuilder/Math` 到首次引用才出现——
  须先有「后台符号名字索引」补回补全覆盖，才不掉体验。与名字索引一起做。
- **后台符号名字索引**（补全立即列出全部 stdlib 符号，优于今天只列已加载包）：独立能力，见 repl.md
  Deferred。
- **残余 eager `Open`（STRS 解码 ×包数）**：本 change 去掉主导的 BuildWorld 后，剩一个更轻的
  O(包数) 头部扫描（建 nsMap 必需）。进一步降到 O(引用) 需延后 Open/STRS 或加全局 ns 清单——另议。
- **SYMS 轻量符号段**（格式变更，让名字索引 O(符号数)）：格式 bump，两-nightly，最后再说。

## Open Questions

- [ ] `LazyReconWorld` 承载于 `DepScanResult`（取代 `Wp[]` 字段）还是独立传参——倾向前者（现 Wp 已在其中）。
