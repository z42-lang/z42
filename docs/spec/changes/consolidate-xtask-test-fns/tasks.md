# Tasks: consolidate-xtask-test-fns

> 状态：🟡 进行中（代码就绪，待 CI 验证）| 创建：2026-07-14

**变更说明：** xtask test 侧超限/重复函数的机械提取（review §2.8 + §2.7）。逐项独立 commit，纯搬移零行为变化。
**原因：** review §2.8（`_testCrossZpkgImpl` ~164 行超函数硬限 60）、§2.7（ios/desktop backend 各自复制的 JUnit-XML 骨架 + `_splitLines` + `_xmlEscape`）。
**文档影响：** 无（纯内部函数提取，不改命令面/行为/机制；`docs/xtask_review.md` §2.7/§2.8 状态由归档时标注）。

## §2.8 拆 cross-zpkg（commit 1 —— 已实施）
- [x] 1.1 新增 `_fixtureDist(pkgDir)`；替换 7 处内联 `Path.Join×3` fixture-dist 路径（含 `_invokeBuildCompiler` outDir）
- [x] 1.2 抽 `_runOneCrossCase(...) -> string`（"" = pass，否则失败标签），逐字搬移 stage 1–4
- [x] 1.3 抽 `_crossSummary(...) -> int`（汇总打印 + rc）
- [x] 1.4 `_testCrossZpkgImpl` 改薄驱动循环；`git diff` 逐路径核对等价（标签/计数/temp 清理序/rc 一致）；三 helper + 主函数均 ≤60 行（53/52/18）

## §2.7 上浮 JUnit 骨架（commit 2 —— 待做）
- [ ] 2.1 `xtask_test_platform.z42` 加 `JUnitCase` 类 + `_writeJUnitReport(root, platform, suite, cases)` + `_stdoutLines` + `_xmlEscape`（骨架 + 转义 + 写盘 + 日志）
- [ ] 2.2 `xtask_test_ios.z42`：`_writeJUnit` 改为解析成 `JUnitCase[]` → 调共享；删本地 `_splitLines`
- [ ] 2.3 `xtask_test_desktop.z42`：同上；删本地 `_splitLines` + `_xmlEscape`
- [ ] 2.4 `git diff` 核对 XML 字节形状不变；两 backend 输出与原一致

## 阶段 3: 验证
- [ ] 3.1 CI 验证（本环境冷启动无 z42 种子、SDK 下载被出网策略挡 403，无法本地跑 `xtask test`；GREEN 判定以 CI 为准 —— bootstrap-seed.md「cold 路径以 CI 为准」）
- [ ] 3.2 CI 绿后归档 + 释放 toolchain 锁

## 备注
- 本环境为全新冷检出：无 z42 种子，nightly SDK 下载 403（组织出网策略），故 z42c 无法本地编译、xtask test 无法本地运行。本重构为纯机械提取，靠 `git diff` 核对等价性；运行时验证交 CI。
- 用户已选：方案 A（我实施 + diff 核对，push 交给 User → CI 验证）；`/loop 持续推进`。
- cross-zpkg 是 GREEN gate stage（`xtask test e2e --dir cross-zpkg`）；JUnit 路径（ios/desktop platform test）非默认 gate、CI 单独腿覆盖，且需 xcodebuild/cc，靠编译 + CI 验。
