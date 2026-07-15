# Tasks: consolidate-xtask-fns-round2

> 状态：🟡 进行中（分节实施，待 CI 验证）| 创建：2026-07-15

**变更说明：** xtask review §2 剩余安全机械收敛项——§2.5（runtime-pkg scaffold）+ §2.7（拷贝变体）+ §2.10（杂项小修）。逐节独立 commit，纯机械/等价，靠 diff 核对 + CI（package/toolchain 腿）验。
**原因：** review §2.5/§2.7/§2.10 列的剩余 dedup/小修。**不含** §2.4（test dist 双构建，行为变更且 test dist 不在 CI）与 §2.6（golden 枚举器，L1 无 lambda/泛型 + CI 拓不到覆盖回退）——两项已在 review 标暂缓，留 warm 环境。
**文档影响：** 无（纯内部；`docs/xtask_review.md` 对应行状态由归档时标注）。

## §2.5 runtime-pkg scaffold（commit 1 —— 已实施）
- [x] 5.1 抽 `_runtimePkgScaffold(root, rid, version, profile) -> pkgDir`→`xtask_stage_components.z42`（runtime-pack 名格式 + native/include+libs 布局单一 SoT）
- [x] 5.2 ios/android/wasm 三处 6-7 行 scaffold 各收敛为 1 行调用（android/wasm 保留 `nativeOut`；android 顺带删提取后不再用的 `rel`；ios/wasm 的 `rel` 另有用途保留）；brace 平衡，无 `pkgName` 泄漏

## §2.7 拷贝变体（commit 2 —— 待核）
- [x] 7.x package/platform zpkg 拷贝变体 vs `_copyAll` —— **复核结论：4 变体均非 `_copyAll` 等价，不收敛**（`_stageCopyExt` 传播异常 vs `_copyAll` 吞 / `_copyNativeLibs` 多条件过滤 / `_pkgCopyLibs` 缺-stdlib `return 1` 守卫 / `_copyZpkgsTo` 预删+计数+日志）。已事实校正标注 review §2.7。

## §2.10 杂项小修（commit 2 —— 已实施）
- [x] 10.1 `_regenCore` 删死参数 `release`（恒 false 未用）+ 唯一调用点 `_regenCore(true)`
- [x] 10.2 `bench stdlib` ✔ 标签随 `kind` 一致（`_procEnd` 原恒 "test stdlib"，与 ▶ "bench stdlib" 不配对）
- [x] 10.3 `xtask_release.z42:116` 裸 `date` 进程 → 调共享 `_utcNow()`（含 ExitCode 兜底）
- [x] 10.4 复核其余 §2.10：`test dist` help/`_testAll(false)`/`_mapFile` 前缀/`_stageToolchain _procEnd` 早已修；`_pkgSha256Check` 改名等低价值项暂留（已在 review §2.10 标注）

## 暂缓项（复核后判定，非本 change 落地）
- §2.4 test dist 双构建：行为变更 + `test dist` 不在 CI → 无验证面，留 warm 环境
- §2.6 golden 枚举器去重：L1 无 lambda/泛型 + CI 抓不到覆盖回退 → 留 warm 环境

## 阶段 2: 验证
- [ ] 2.1 CI 验证（冷检出无法本地跑；GREEN 以 CI 为准）——关键腿：package-ios/android/wasm（scaffold）+ compile-toolchain；已推 main（9a707bff §2.5 / 6b5ee43e §2.10）
- [ ] 2.2 CI 绿后归档 + 释放 toolchain 锁；红则 revert 对应 commit

## ⚠️ §2.5 回归 + 热修（2026-07-15）
- `9a707bff` 误删 `xtask_package_android.z42` 的 `bool rel`（判为提取后未用），但它仍被 47/63/77 的 per-ABI cargo `--release` 门用 → `E0401: undefined: rel` 让 **compile-toolchain 在 main 全红**（run 29427650069/29428595684），并行 session 的 push 亦继承此红。
- 根因：删「未用」变量前的 grep 用了带 `-vE` 过滤的管道，漏掉真实引用。**教训：删变量必须整文件 grep 裸标识符**（冷环境不能本地编译，放大风险）。
- 热修 `536bcb58` 恢复 `bool rel`（ios/wasm 当初保留其 `rel` 未受影响）。盯 compile-toolchain 转绿。

## ⚠️ 并行 session 冲突记录（2026-07-15）
- 原 §2.5 commit `fead63ff` 因并行 session（`migrate-stdlib-to-params` / 0.4.0 stream）重写 main（版本 bump + 归档）被**孤立**；已从 orphan 提取代码等价重落为 `9a707bff`。
- ACTIVE.md / 工作树被两 session 并发编辑——全程显式 `git add` 只纳本 change 文件、未触其 WIP。

## 备注
- 直连 main 开发（User 指示）；纯机械/等价，push 后盯 CI，红即 revert。
- 共享工作树有并行 session 的 `migrate-stdlib-to-params` WIP（stdlib/runtime）——本 change 仅动 `scripts/package/`+`scripts/test/`（toolchain），子系统不冲突；commit 一律显式 `git add`、不用 `-A`。
