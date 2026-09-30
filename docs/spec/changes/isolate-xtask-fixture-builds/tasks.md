# Tasks: 测试夹具拷进 artifacts/tmp 再编（cross-zpkg / multi-exe / manifest-targets / z42b）+ gc-modes 产物进 tmp

**状态：🟡 进行中 | 开始：2026-09-30**

类型：`refactor`（只改 xtask，不改 z42c / z42b，用户不可见）→ 最小化模式。叠在 `tidy-artifacts-tmp-and-clean`（#969）之上。

「源码树零写入」系列第 1 步。源码树产物的来源盘点（O1–O11）与后续步骤见本 change 的 PR 描述；
本步只改 xtask：O1–O4（cross-zpkg / multi-exe）、O10（manifest-targets / z42b 夹具）、O6 的 dist（gc-modes）。

## 动机

夹具工程没有 `[build]`，z42c 按单工程默认布局把 dist / cache / generated 写在工程目录旁；xtask
还在旁边建 `<pkg>/libs/` 中转目录。一轮 GREEN 后 `src/tests/{cross-zpkg,multi-exe}` 下 ~650 个产物目录。
另一个真实代价：上一轮留在源码树里的旧格式 zpkg 会赢过本轮产物（2026-09-27 实测 52 个 cross-zpkg 假红）。

## 进度概览

- [x] `xtask_fs.z42`：`_stageFixtureTree(root, srcRel, name)` —— 重置 `tmp/<name>` 并拷入夹具源码树（跳过 artifacts/ dist/）
- [x] cross-zpkg / multi-exe 入口的 `testsDir` 改指拷贝（其余路径全相对 testsDir，无需改动）
- [x] 文档：artifacts-layout.md §3 表格 + §4 源码树残留说明
- [x] manifest-targets / z42b：`_testTargetsCore` 开头把两棵树各拷一次进 tmp（整个 run 共用 —— reuse-parent
      等 smoke 要看前面 smoke 留下的父包产物，不能逐个重置），路径经 `TargetTooling.{FixturesRoot,Z42bFixturesRoot}` 下传
- [x] gc-modes：`--output-dir tmp/gc-modes`（dist 不再落 `src/compiler/z42c.semantics/dist`；cache 待 z42c 修）
- [x] 本地：`test e2e --dir cross-zpkg` 89/89、`--dir multi-exe` 3/3
- [x] 完整 GREEN（9m03s，全阶段通过）。跑完 **`src/tests` 下 0 个产物目录**（此前 ~650）；源码树剩 27 处，全在
      stdlib / 编译器成员目录，分属后续步骤：自举预建与 gc-modes 的 cache（z42c `--output-dir` 修复）、z42b dev 目标
      输出（z42b `--out-root`）、path 依赖 dist（path 依赖布局决策）、repl hooks（z42b hooks 落点修复）
- [ ] PR CI + 归档

## 为什么是「拷贝」而不是给 z42c 传 `--output-dir`

`--output-dir` 在单工程下只改 dist，cache / generated 仍写工程目录旁（且只写不读）—— 那是 z42c 的 bug，
另立 change 修。拷贝方案对任何 z42c（含种子）都立即有效，并且每轮从零开始。
