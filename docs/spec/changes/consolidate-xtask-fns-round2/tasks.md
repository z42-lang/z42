# Tasks: consolidate-xtask-fns-round2

> 状态：🟡 进行中（分节实施，待 CI 验证）| 创建：2026-07-15

**变更说明：** xtask review §2 剩余安全机械收敛项——§2.5（runtime-pkg scaffold）+ §2.7（拷贝变体）+ §2.10（杂项小修）。逐节独立 commit，纯机械/等价，靠 diff 核对 + CI（package/toolchain 腿）验。
**原因：** review §2.5/§2.7/§2.10 列的剩余 dedup/小修。**不含** §2.4（test dist 双构建，行为变更且 test dist 不在 CI）与 §2.6（golden 枚举器，L1 无 lambda/泛型 + CI 拓不到覆盖回退）——两项已在 review 标暂缓，留 warm 环境。
**文档影响：** 无（纯内部；`docs/xtask_review.md` 对应行状态由归档时标注）。

## §2.5 runtime-pkg scaffold（commit 1 —— 已实施）
- [x] 5.1 抽 `_runtimePkgScaffold(root, rid, version, profile) -> pkgDir`→`xtask_stage_components.z42`（runtime-pack 名格式 + native/include+libs 布局单一 SoT）
- [x] 5.2 ios/android/wasm 三处 6-7 行 scaffold 各收敛为 1 行调用（android/wasm 保留 `nativeOut`；android 顺带删提取后不再用的 `rel`；ios/wasm 的 `rel` 另有用途保留）；brace 平衡，无 `pkgName` 泄漏

## §2.7 拷贝变体（commit 2 —— 待核）
- [ ] 7.x package/platform zpkg 拷贝变体 vs `_copyAll` —— 逐个核错误语义（`_copyIfExists` 差异），仅等价者收敛

## §2.10 杂项小修（commit 3 —— 待核）
- [ ] 10.x 复核 §2.10 剩余项当前状态，落地清晰安全的（1 行修）

## 阶段 2: 验证
- [ ] 2.1 CI 验证（冷检出无法本地跑；GREEN 以 CI 为准）——关键腿：package-ios/android/wasm（scaffold）+ compile-toolchain
- [ ] 2.2 CI 绿后归档 + 释放 toolchain 锁；红则 revert 对应 commit

## 备注
- 直连 main 开发（User 指示）；纯机械/等价，push 后盯 CI，红即 revert。
- 共享工作树有并行 session 的 `migrate-stdlib-to-params` WIP（stdlib/runtime）——本 change 仅动 `scripts/package/`+`scripts/test/`（toolchain），子系统不冲突；commit 一律显式 `git add`、不用 `-A`。
