# tasks：fix-crosspkg-ns-reachability

状态：🟢 已完成（2026-09-25）

## 代码
- 🟢 **nsMap 改多值**：`DepScan._nsPairIndexOf`（新）取代按 ns 判重；三个构建点
  （`DepScan.ScanDirs` ×2 / `DepScan.ExtendWithPackage` / `NsIndexCache` 缓存路径）同步。
  ⭐ **`FileOf(ns)` 仍返回第一行** ⇒ 路由语义逐字不变。
- 🟢 `ZpkgBuilder._addProviders`（取代 `_addPair`）：一个 ns 的**全部**提供包进 DEPS。
  顺带把扩容搬进新的 `ZpkgDepPairs.Add`（调用方此前抄了两遍，且只保证「再放得下一个」——
  一个 ns 带出多个提供包后那个假设不成立）。
- 🟢 **E0497**（新码）：`SymbolTable.UndeclaredDepMsg` + 三件套字段
  （`DeclaredDeps` / `SelfPkg` / `EnforceDeclaredDeps` + 去重集 `_undeclaredSeen`），
  经 `ImportedSymbols` 搭车 → `SymbolCollector._mergeImports` 并入；两个消费端挂在既有
  `_chkTypeRefPkg`（声明位 / 使用位）。driver 的 manifest 路径打开开关。
- 🟢 **范围只管第三方**（`pkg.StartsWith("z42.")` 放行）。

## 验证
- 🟢 四档行为：单文件 `Std.Collections` ✅ / 单文件 `Std.Text` ✅ / 工程无 deps ✅ /
  工程声明后 ✅（修前第一、三档运行期 `MissingSymbolException`）。
- 🟢 **E0497 可达性已钉死**（这条差点交付成一个恒不响的门）：唯一能触发的形状是
  「包在解析域里、却没被声明」。未声明的第三方包压根进不了解析域（E0443 先报），
  真正的场景是**传递依赖**——`app → acme.web → acme.util`（均 path 依赖），app 直接用
  `Acme.Util.Helper` 却没声明 `acme.util` → **E0497**；声明后 → `44`。两侧都有判别力。
- 🟢 用例 `src/tests/cross-zpkg/undeclared_dep/`（`expected_build_error.txt` = `E0497`）。
  选这套 harness 是因为 `_prepPkgLibs` **无条件**把 target 的 dist 拷进 `main/libs`，
  与声明与否无关 —— 正是上面那个形状。
- 🟢 `xtask test all` / `examples` / `diagcodes` / `docs` / `lines` / `walkers` 全部 exit 0。
- 🟢 ① 单独跑过一轮完整 `test all`（零回归）后才叠 ②。

## 文档（三处冲突归一）
- 🟢 `collections.md`：删「必须声明」「单文件用不了本包」，改为「自动可用」+ 📜 说明真相。
- 🟢 `z42-toml.md`：删已不存在的 **WS013**，写明 `[dependencies]` 只写第三方 + 漏写报 E0497。
- 🟢 `error-codes.md`：E0497 词条（含「判据是归属包不是 using」「stdlib 不在管辖内」）。

## ⭐ 记下来的五条
- ⭐⭐ **记录的边界又窄了一层**：坑点清单记的是「单文件加载不到跨包 ns 的后半边」，
  实测**任何没写 `[dependencies]` 的工程**同样崩，且**声明与否几乎无关**——
  真正起作用的是「命名空间有没有被 `z42.core` 抢先占住」。
- ⭐⭐ **初稿正面推翻了一条已归档的设计**。`simplify-stdlib-auto-import`（2026-06-06）确立
  Rust-std 模型「stdlib 自动可用、不要声明」，我却在实现「stdlib 不声明就报错」。
  ⇒ **动「依赖/包模型」这类横切规则前，先 `grep docs/spec/archive` 找有没有人裁过。**
- ⭐ **诊断的判据选错一个维度就会假阳性爆炸**：按 `using` 判 → 全仓 299 条命中、约 250 条来自
  `using Std;`（11 个包共享该 ns）；按**符号归属包**判 → 3 条，收窄后 0 条。
- ⭐ **「零命中」必须解释**：收窄后仓内 0 命中，我据此怀疑它是恒不响的门，去造 fixture 才发现
  它只在**传递依赖**这一形状下可达。**先前我造不出来，是因为未声明的第三方包进不了解析域。**
- ⚠️ **WS013 是「文档在、实现没了」的又一例**（随 C# 编译器蒸发）。读到「会触发 XXX 警告」
  这类文档断言，先 `grep` 一下它还在不在。
