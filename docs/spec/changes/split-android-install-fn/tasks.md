# Tasks: split-android-install-fn

> 状态：🟡 进行中（代码就绪，待 CI 验证）| 创建：2026-07-15

**变更说明：** 拆 `_depsInstallAndroidSdk`（~161 行超函数硬限 60，review §2.8）为「preamble + 逐步骤 helper」。纯机械搬移零行为变化。
**原因：** review §2.8 表列「`_depsInstallAndroidSdk` `xtask_install_android.z42:27-187` ~161 | 按 [1]-[6] 步骤各提一函数」。
**文档影响：** 无（纯内部函数提取，不改命令面/行为/机制；`docs/xtask_review.md` 对应行状态由归档时标注）。

## §2.8 拆 android install（commit 1）
- [x] 1.1 抽 `_androidResolveJdk() -> string`（JDK 17+ 解析块，返 javaHome 或 "" 并已打印错误）
- [x] 1.2 抽 `_androidCmdlineTools(force, sdkDir, cmdlineUrlTmpl, hostOs, cmdlineSha) -> int`（步骤 [1]，skip 分支返 0）
- [x] 1.3 抽 `_androidSdkPackages(wantEmu, sdkmanager, sdkRoot, platformVer, buildToolsVer, sysImage, javaHome) -> int`（步骤 [3]）
- [x] 1.4 抽 `_androidNdk(sdkmanager, sdkRoot, ndkVer, sdkDir, ndkDirLink, javaHome) -> int`（步骤 [4]）
- [x] 1.5 抽 `_androidEmulatorExtras(wantEmu, force, avdmanager, avdName, avdDevice, sysImage, gradleDir, gradleVer, gradleSha, hostOs, javaHome) -> int`（步骤 [5]+[6]）
- [x] 1.6 抽 `_androidPrintExports(wantEmu, sdkDir, ndkDirLink, javaHome, gradleInstall)`（结尾 export 汇总）
- [x] 1.7 `_depsInstallAndroidSdk` 改薄驱动（preamble + 6 helper 调用），主函数 **~161→49 行**；`git diff` 逐路径核对控制流等价（skip/日志/rc/temp 序一致 + 全 user-facing 字符串 sorted-diff 零差异 + 无变量泄漏 + 6 helper 各调用一次）
  - 注：文件 271→308 行，**越 300 软限 8 行**（advisory，非硬限）——6 helper 的签名/注释固有开销所致；权衡上**消一个 161 行硬限函数违规 > 守一个 8 行软限**（code-organization：软限「建议拆分不强制」，硬限「必须」）。已删冗余注释压至最小溢出。

## 阶段 2: 验证
- [ ] 2.1 CI 验证（本环境冷启动无 z42 种子、SDK 下载被出网策略挡 403，无法本地跑 `xtask` 编译/测试；GREEN 判定以 CI 为准 —— bootstrap-seed.md「cold 路径以 CI 为准」）
- [ ] 2.2 CI 绿后归档 + 释放 toolchain 锁

## 备注
- 本环境为全新冷检出：无 z42 种子、nightly SDK 下载 403（组织出网策略），故 z42c 无法本地编译、xtask 无法本地运行。纯机械提取，靠 `git diff` 核对等价性；编译正确性由 CI 的 xtask build 检、运行行为由 diff 保证。
- `_depsInstallAndroidSdk` 是 **dev-only 路径**（文件头注：CI 不跑，装 cargo-ndk inline），CI 只编译不执行 → 运行时行为无 gate 覆盖，diff 核对是唯一保证。
- 延续已归档 `consolidate-xtask-fns`（§2.5/§2.7/§2.8）的机械提取手法与验证策略（方案 A：实施 + diff 核对，CI 验证）。
