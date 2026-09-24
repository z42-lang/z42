# proposal：fix-crosspkg-ns-reachability

## 一句话

「一个包能不能用」今天由一条**没人写下来、也没人打算要**的规则决定——**该包的命名空间有没有被
`z42.core` 抢先占住**（nsMap first-wins）。把它换成已经裁决过的那条规则：
**标准库自动可用（含单文件）；`[dependencies]` 只写第三方，漏写编译期报 E0497**。

## 今天的真实规则（四组实测）

`z42.text` 与 `z42.collections` 都是非 prelude 的标准库包，地位对等：

| 配置 | `Std.Text`（唯一提供者） | `Std.Collections`（core 也声明） |
|---|---|---|
| 工程 + 声明了该包 | ✅ | ✅ |
| 工程 + **未声明** | ✅ **照样能跑** | 🔴 运行期 `MissingSymbolException` |
| 工程 + 声明了**别的**包 | ✅ | 🔴 同上 |
| 单文件（无法声明） | ✅ | 🔴 同上 |

⇒ **「必须声明」这条规则根本没被执行**；真正起作用的是 nsMap 的 first-wins。
`docs/reference/src/stdlib/collections.md:15` 写的「本包不是隐式依赖，工程清单里必须写上」
今天是一句**空话**——不写也能用，只要你的命名空间没被 core 占。

## 🔴 规范冲突：**三处互相矛盾**（本 change 一并裁掉）

| 出处 | 说法 |
|---|---|
| `z42-toml.md:65` + 归档设计 `simplify-stdlib-auto-import` | `z42.*` **始终可用，不要声明**（声明会触发 **WS013 警告**）——Rust-std 模型 |
| `collections.md:15` | 本包不是隐式依赖，**工程清单里必须写上** |
| `collections.md:22` | **单文件脚本用不了本包**，要建工程 |
| `launcher.z42:118`（`z42 run --help`） | 单文件 can only use the standard library（读作整个标准库可用） |

**以已归档的 Rust-std 模型为准**：标准库自动可用（含单文件），`[dependencies]` 只写第三方。
`collections.md` 那两段改掉（它把实现缺陷写成了规则）。

⚠️ **`WS013` 已经不存在了**：它是 C# 编译器时代的 lint（`ManifestErrors.cs` / `ProjectManifest.cs`），
随 2026-06-26 移除 C# 编译器一起蒸发，自举实现从未补回（判据：`grep WS013 src/` 为空），
而 `z42-toml.md` 至今还写着它会警告。同族于 E0407 / `FlowAnalyzer.cs` 那类「常量在、文档在、
发它的 pass 不在」。本 change 只订正文档（不重建 WS013：冗余声明无害，不值得为它新增一道门）。

## 根因

DEPS（运行期惰性加载的候选包名单）= **nsMap 的 first-wins 结果** ∪ **manifest 声明的依赖**：

- `DepScan` 的 nsMap 是 `ns → 单个 zpkg`，**first-wins**（`DepScan.z42:105`），prelude 排序在前
  ⇒ `Std.Collections` 恒映射到 `z42.core.zpkg`，`z42.collections.zpkg` 永远进不了 DEPS。
- `PackageCompile._unionDeclaredDeps` 是**针对这条的既有补救**（其注释明写「命名空间跨包时
  first-wins 只记第一个提供包……补救：把 toml 声明的依赖并入 DEPS」）。
- ⇒ 没有 `[dependencies]` 的工程与**单文件**（合成清单根本没有该段）拿不到这份补救。

受影响的分裂命名空间：`Std`（**11 个包**都声明它）、`Std.Collections`、`Std.IO`、
`Std.Threading`、`Std.Net.Sockets`。

## 改动（五部分）

### ① nsMap 改多值（根因）

`NsNames`/`NsFiles` 平行数组**允许同一 ns 出现多行**（每个提供包一行，按既有 sorted 顺序追加）：

- `DepScan` / `NsIndexCache` 的去重判据从「ns 已存在」改为「(ns, file) 对已存在」；
- `ZpkgBuilder._addPair` 命中一个 ns 时加入**全部**提供包，不再只取第一个；
- ⭐ **`DepScanResult.FileOf(ns)` 仍返回第一行** ⇒ 路由语义逐字不变、零字节漂移风险集中在 DEPS。

### ② 未声明**第三方**依赖的编译期诊断（**E0497**，新码）

🔴 **范围只管第三方包**（User 裁决）。初稿把 stdlib 也算进去，那会**正面推翻一条已归档的设计**：
`simplify-stdlib-auto-import`（2026-06-06）确立的 **Rust-std 模型——标准库自动可用，`[dependencies]` 只写第三方**。
实施时才挖出它，已按它收窄（`pkg.StartsWith("z42.")` 一律放行）。

**必须按「符号的归属包」判，不能按 `using` 判。** 实测：按 using 扫全仓得 **299 条命中，其中约 250 条来自
`using Std;`**——`Std` 一个命名空间由 **11 个包**共同声明。按归属包（`SymbolTable.ClassPkgAll`）判，
收窄前全仓只命中 **3** 处（都是 stdlib，收窄后归零）。

挂在既有的 `_chkTypeRefPkg`（声明位 + 使用位两个消费端，已经是类型引用的 choke point）。
每个缺失的包**只报一次**（去重键是包名：用户要做的动作只有加那一行）。

⚠️ **它唯一能触发的形状是「包在解析域里、却没被声明」**。未声明的第三方包压根进不了解析域
（E0443 先报），所以真正的场景是**传递依赖**：`app → acme.web → acme.util`，app 直接用了
`Acme.Util.Helper` 却没声明 `acme.util`。那正是经典的**传递依赖泄漏**。
实测两侧都有判别力：未声明 → E0497；声明后 → 编运行均正常。

### ③ 单文件

**不需要任何特殊处理**。① 已经让单文件拿到全部提供包；② 又不管 stdlib。
初稿曾让 launcher 把 SDK libs 写进合成清单的 `[dependencies]`，收窄 ② 后那段**已撤销**。

### ④ 仓内命中：收窄后归零

收窄前实测命中 **3** 处（`z42.toml` / `z42.yaml` / `z42.net` 都是用了 `z42.text` 的
`StringBuilder` 没声明）+ 3 个测试工程用 `z42.ir`。按「stdlib 自动可用」收窄后，
这 6 处**全部撤回**——它们本就是那条已归档设计说的「冗余声明」。仓内真命中：**0**。

### ⑤ 文档

- `collections.md` 删掉「单文件用不了本包」，改写成真实规则；
- `z42-toml.md` / `generic-constraints.md` 等交叉引用同步；
- `error-codes.md` 加 E0497 词条。

## 风险与边界

- ✅ **②的爆炸半径已量**：收窄到第三方后，仓内命中 **0**。
- 🔴 **另外挖出一个真 bug，本 change 不做**：**path 依赖的传递闭包没被装配**。
  `app → acme.web → acme.util`（均 path 依赖）时，`app/dist/` 里只有 `acme.web.zpkg`、
  没有 `acme.util.zpkg` ⇒ 运行期 `MissingSymbolException: Acme.Util.Helper`。
  决定性实验：手工拷入即打出 `42`。与本 change 不同机制（装配 vs 名字解析），
  `Acme.Util` 还是唯一命名空间，① 修不到。已记入坑点清单。
- ⚠️ ①会给「用了分裂 ns 却没声明」的包**增加 DEPS 条目** ⇒ zpkg 内容变化，
  `xtask test fingerprint` 可能要求 bump（本 change 内确认）。
- 🔴 **`FileOf` 必须保持 first-wins**：它是路由（`DepScan.z42:391`）与 REPL 补全的判据，
  改它会波及远超本 change 的范围。
