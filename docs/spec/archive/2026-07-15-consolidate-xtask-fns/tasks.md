# Tasks: consolidate-xtask-fns

> 状态：🟢 已完成 | 创建：2026-07-14 | 完成：2026-07-15

**变更说明：** xtask 超限/重复函数的机械提取收敛（review §2.5 + §2.7 + §2.8）。逐项独立 commit，纯搬移零行为变化。
**原因：** review §2.8（`_testCrossZpkgImpl` ~164 行超函数硬限 60）、§2.7（ios/desktop backend 复制的 JUnit-XML 骨架 + `_splitLines` + `_xmlEscape`）、§2.5（ABI headers 拷贝对 6 处重复）。
**文档影响：** 无（纯内部函数提取，不改命令面/行为/机制；`docs/xtask_review.md` 对应节状态由归档时标注）。

## §2.8 拆 cross-zpkg（commit 1 —— 已实施）
- [x] 1.1 新增 `_fixtureDist(pkgDir)`；替换 7 处内联 `Path.Join×3` fixture-dist 路径（含 `_invokeBuildCompiler` outDir）
- [x] 1.2 抽 `_runOneCrossCase(...) -> string`（"" = pass，否则失败标签），逐字搬移 stage 1–4
- [x] 1.3 抽 `_crossSummary(...) -> int`（汇总打印 + rc）
- [x] 1.4 `_testCrossZpkgImpl` 改薄驱动循环；`git diff` 逐路径核对等价（标签/计数/temp 清理序/rc 一致）；三 helper + 主函数均 ≤60 行（53/52/18）

## §2.7 上浮 JUnit 骨架（commit 2 —— 已实施）
- [x] 2.1 `xtask_test_platform.z42` 加 `JUnitCase` 类 + `_writeJUnitReport(root, platform, suite, cases)` + `_stdoutLines` + `_xmlEscape`（骨架 + 转义 + 写盘 + 日志），全仓唯一定义
- [x] 2.2 `xtask_test_ios.z42`：`_writeJUnit` 两遍扫描建 `JUnitCase[]` → 调共享；3 处 `this._splitLines`→`_stdoutLines`；删本地 `_splitLines`
- [x] 2.3 `xtask_test_desktop.z42`：同上；删本地 `_splitLines` + `_xmlEscape`
- [x] 2.4 `git diff` 核对：suite==classname 代入后 XML 逐字节同形；写盘路径/日志/计数一致（ios 失败 msg 恒 "failed"、desktop msg 逻辑不变）；brace/paren 平衡；platform 255<300 软限

## §2.5 ABI headers 拷贝对（commit 3 —— 已实施）
- [x] 5.1 `xtask_stage_components.z42` 加 `_copyAbiHeaders(root, destIncludeDir)`（`src/runtime/include/z42_{abi,host}.h` → `<destIncludeDir>/`，源路径单一 SoT）
- [x] 5.2 替换 6 处 `File.Copy` 对（android×2 / ios×2 / wasm / stage_components）；dest 字符串逐字节等价（`Path.Join(dir,"native/include/z42_abi.h")` == `Path.Join(Path.Join(dir,"native/include"),"z42_abi.h")`）；创建 include 目录的调用未动 → File.Copy 语义不变；brace/paren 平衡

## 阶段 3: 验证
- [x] 3.1 CI 验证（本环境冷启动无 z42 种子、SDK 下载被出网策略挡 403，无法本地跑 `xtask test`；GREEN 判定以 CI 为准 —— bootstrap-seed.md「cold 路径以 CI 为准」）→ **PR #6（dbcde969）已合并 main，CI 全绿**
- [x] 3.2 CI 绿后归档 + 释放 toolchain 锁（本次会话）

## 备注
- 本环境为全新冷检出：无 z42 种子，nightly SDK 下载 403（组织出网策略），故 z42c 无法本地编译、xtask test 无法本地运行。本重构为纯机械提取，靠 `git diff` 核对等价性；运行时验证交 CI。
- 用户已选：方案 A（我实施 + diff 核对，push 交给 User → CI 验证）；`/loop 持续推进`。
- cross-zpkg 是 GREEN gate stage（`xtask test e2e --dir cross-zpkg`）；JUnit 路径（ios/desktop platform test）非默认 gate、CI 单独腿覆盖，且需 xcodebuild/cc，靠编译 + CI 验。
