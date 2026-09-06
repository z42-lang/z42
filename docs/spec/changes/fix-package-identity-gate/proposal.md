# Proposal: package 一致性门补齐 —— 表驱动的「包内副本 vs 仓库源」逐字节校验

## Why

`xtask package` 末尾跑的一致性门有两个问题，一个是名，一个是**假保障**。

### 1. 名不副实

`_pkgSha256Check`（`scripts/package/xtask_package.z42:278`）**一个哈希都不算**，是
`_filesEqual` 读两个文件逐字节比较。但函数名、区块注释、以及三行**用户可见输出**
（`SHA-256 invariant check:` / `✗ SHA-256 invariant FAILED` / `✓ SHA-256 invariants OK`）
全在说 SHA-256。

溯源：`add-host-package-conform` 的 design D4 里，原始 bash `package.sh` 的 `pkg_sha256_check`
**确实**算 sha256sum。移植到 z42 时实现换成了直接字节比较（**更强、更简单**——不用外部工具、
无碰撞面），名字留了下来。

### 2. 文档承诺了代码没做的事

[`docs/workflow/packaging.md` §4](../../../workflow/packaging.md) 声称这个门覆盖 6 类文件、
保证「跨 9 包 byte-identical」。实际：

| 文档声称 | 代码实况 |
|---|---|
| 跨 9 包 byte-identical | ❌ 只比**包 vs 仓库源**；`_filesEqual` 只在 `xtask_package.z42` 内部用，**全仓无任何跨包比对** |
| `libs/*.zpkg` | ✅ 有 |
| `native/include/{z42_abi,z42_host}.h` | ✅ 有 |
| `examples/hello_c/main.c` | ❌ **package 侧根本没拷 `examples/`**，包里没有这个文件 |
| iOS `Sources/Z42VM/*.swift` 跨 slice | ❌ 无（`_packageIos:95` 拷了，没人校） |
| Android `*.kt` + `cpp/z42vm_jni.c` 跨 ABI | ❌ 无（`_packageAndroid:182,183,200` 拷了，没人校） |
| wasm `js/{index.js,index.d.ts,stdlib-resolver.js}` | ❌ 无（`_packageWasm:87` 拷了，没人校） |

archive 的 D4 说跨包比对「在 CI release 阶段做」——CI 里 grep 不到。

**后果**：读文档的人以为跨包一致性有门守着，实际 platform 侧四类源码副本一个没查。

## 关键论点：不要去实现「跨包比对」

**「跨包 byte-identical」是「每包 vs 单一仓库源」的自动推论，且后者更强。**

上表里每一类文件，各个包里的副本都是从**仓库里同一份源**拷进去的
（`iosSrc/Sources/Z42VM/*.swift`、`proj/z42vm/src/main/cpp/*`、`platforms/wasm/js/*`、
flat stdlib dist、`src/runtime/include/*.h`）。因此：

> A == 源 ∧ B == 源 ⟹ A == B

而反过来不成立：两两比对只能证明「大家一样」，**证明不了「大家都对」**——所有包一致地拷了
一份陈旧副本，跨包比对全绿。**「vs 单一源」能抓到这种情形，跨包比对抓不到。**

而且跨包比对在当前拓扑下**做不了**：`package sdk` / `package runtime --rid X` 各自是独立
xtask 进程，CI 的 `package-{android,ios,wasm}` 是**不同 job、不同 runner**，`_pkgFinish`
永远只见得到一个 pkgDir。真要两两比对得改 CI 拓扑（汇总各 job 的 artifact 再比），
为一个**更弱**的性质付这个成本不划算。

> 这一条与 User 选定的「补齐跨包门」在**目标**上一致（跨包一致性有门守着），
> 在**手段**上更省更强。若 User 认为仍需真·跨包比对（例如防「同一源在不同 runner 上被
> 不同工具链改写」），请指出，我另开一节设计 CI 汇总方案。

## What Changes

### A. 表驱动的一致性门（把 4 类漏检补上）

现有实现是「libs 一段 + 两个 header 各一个 `if`」的硬编码链，platform 侧要补 4 类就会变成
一长串 `if`。改成**声明式表**：

```
_PkgIdentityRule { string PkgRel; string SrcRel; string Pattern; }   // 包内相对路径 / 仓库源相对路径 / glob（空=单文件）
```

一张 `_pkgIdentityRules()` 表列出全部规则，`_pkgSourceIdentityCheck` 统一遍历：
**包内不存在该路径 → 跳过**（一张表服务全部包类别：desktop 没有 `Sources/`，ios 没有 `js/`）。

| 规则 | 包内 | 仓库源 | 现状 |
|---|---|---|---|
| stdlib | `libs/*.zpkg` | `_libsDir(root)` | 已有 |
| C ABI 头 | `native/include/z42_abi.h`、`z42_host.h` | `src/runtime/include/` | 已有 |
| iOS Swift | `Sources/Z42VM/*.swift`、`Sources/Z42VMC/dummy.c` | `platforms/ios/Sources/…` | **新增** |
| Android JNI | `cpp/z42vm_jni.c`、`cpp/CMakeLists.txt` | `…/z42vm/src/main/cpp/` | **新增** |
| Android Kotlin | `kotlin/**/*.kt` | android 工程源 | **新增** |
| wasm JS | `js/index.js`、`index.d.ts`、`stdlib-resolver.js` | `platforms/wasm/js/` | **新增** |

> 精确的源路径以 `_packageIos` / `_packageAndroid` / `_packageWasm` 里**实际拷贝的那一行**为准
> （规则表与拷贝点必须同源，否则门会验一个不相干的文件）。实施时逐条对照写。

### B. 诚实化命名

- `_pkgSha256Check` → `_pkgSourceIdentityCheck`
- 用户可见输出：`SHA-256 invariant check:` → `source-identity check (package copies vs repo source):`；
  `✓ SHA-256 invariants OK` → `✓ source-identity OK`；FAILED 同理
- `scripts/README.md` ×2、`docs/workflow/packaging.md` §4 同步

### C. `examples/hello_c/main.c` —— 删文档，不补包（待 User 确认）

文档说包里有它，实际**任何包都没拷 `examples/`**。两条路：

- **(建议) 从文档删**：`examples/embedding/hello_c/` 在仓库里存在、是给读者看的嵌入示例；
  它没进包是 2026-05-13 之后包结构重构（`sdk`/`runtime`/`workload-desktop` 三包制，
  已不是当年的 9/13 包）留下的**文档残留**，不是功能退化。
- (备选) 真拷进 SDK 包：SDK 已带 `native/include/*.h`，配一个 C 嵌入示例合理——但这是
  **新增包内容 = 行为变更**，应单列一个 change，不混进本次「补门」。

**本 proposal 按「从文档删 + 登记为可选增强」写；User 若要 (备选) 请指出。**

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/package/xtask_package.z42` | MODIFY | `_pkgSha256Check`→`_pkgSourceIdentityCheck` 表驱动重写；`_pkgFinish` 文案 |
| `scripts/package/xtask_package_ios.z42` | 只读引用 | 确认 swift/dummy.c 的源路径 |
| `scripts/package/xtask_package_android.z42` | 只读引用 | 确认 jni/CMakeLists/kt 的源路径 |
| `scripts/package/xtask_package_wasm.z42` | 只读引用 | 确认 js 的源路径 |
| `docs/workflow/packaging.md` | MODIFY | §4 重写：改名 + 覆盖面据实 + 讲清「vs 源 ⟹ 跨包一致且更强」 |
| `scripts/README.md` | MODIFY | 两处 `SHA-256 invariant` 表述 |
| `docs/spec/changes/fix-package-identity-gate/` | NEW | 本 proposal + tasks.md |

**不改**：`docs/spec/archive/**`（历史记录，记的是当时的决定）。

## 验证计划

1. **正例**：`xtask package sdk --no-build` → source-identity 全绿（含新增规则里 desktop 包实际存在的部分）。
2. **负例（关键——新增的规则必须真的会红）**：临时篡改包内某个副本（如 `native/include/z42_abi.h`
   加一个字节），重跑 → 必须精确报出该文件并 exit 1。**每类新增规则各做一次**，否则等于加了
   一堆永远不执行的死规则（正是本次要修的那种「假保障」）。
   > platform 包（ios/android/wasm）本机不一定打得出来；打不出的类别用**构造一个假 pkgDir**
   > （按规则表铺出对应文件）直接调 `_pkgSourceIdentityCheck` 验证正/负例。
3. `xtask test` 全绿 10/10 stage。
4. **`test packages` 补跑**（packaging 自检层，不在 GREEN gate 内）。

## 状态

📝 DRAFT — 等 User 确认后进 IMPL。**需确认两点**：① 「不做真·跨包比对」的论证是否接受；
② `examples/hello_c/main.c` 走「从文档删」还是「真拷进 SDK 包」。
