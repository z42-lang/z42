# Tasks: 构建/清理输出里的路径不体面

> 状态：🟢 已完成 | 创建：2026-09-16 | 完成：2026-09-16 | 类型：fix（最小化模式）

**变更说明：** `z42 build` 的 `cache -> …` 与 `z42 clean` 的 `removed …` 打的是**绝对路径且含 `/./`**，
同一屏里与相对的 `wrote -> ./dist/…` 并列，观感割裂。改为规范化 + 相对当前目录呈现。

**原因：** 学习手册第 3 章（`add-single-file-run` 刚落地）会把这两条输出直接展示给读者，是教程里唯一一处
「输出不体面」的地方；transcript 也只能用通配符糊掉真实内容，违反 learn-writing「尽量少用通配符」。

**现状实测（main 0b4ba37d7）：**

```
$ z42 build
cached: 0/1 files
wrote -> ./dist/greeter.zpkg (indexed, 1 zbc)          ← 相对，好看
cache -> /private/tmp/…/greeter/./artifacts/debug/.cache (1 files)   ← 绝对 + /./

$ z42 clean
removed ./dist                                          ← 相对
removed /private/tmp/…/greeter/./artifacts              ← 绝对 + /./
```

**根因（两条，叠加）：**

1. `BuildLayout.Resolve` 里 `abs = Path.IsRooted(projectDir) ? projectDir : Path.Join(cwd, projectDir)`。
   单工程下 `projectDir == "."` ⇒ `abs == "<cwd>/."` —— **`/./` 由此而来**，并顺着 `${workspace_dir}`
   模板渗进 `OutputDir` / `CacheDir` / `GeneratedDir`，以及 `clean` 的 `Path.Join(l.ProjectDir, "artifacts")`。
   `Path.Join` 是纯词法拼接，不做规范化（这是刻意的，不改它）。
2. `DistDir` 在「既无 output_dir 也无 dist_dir」的默认分支走的是 `Path.Join(projectDir, "dist")`（**原始**
   相对 projectDir），而其余三个目录走 `_expand` → 一律绝对。同一个 BuildLayout 里一半相对一半绝对，
   打印出来自然割裂。

**文档影响：** `docs/book/src/toolchain/cli.md`（若示例输出含这两行）；学习手册第 3 章的
`examples/getting-started/projects/greeter/run.console` 里的 `[..]` 通配符可以换成真实输出。

## 阶段 1：规范化（消灭 `/./`）
- [x] 1.1 `src/libraries/z42.core/src/IO/Path.z42` 新增 `Path.Normalize(string)`：折叠 `//`、去掉 `.` 段、
      词法解析 `..`、去尾 `/`（根 `/` 保留）。**不改 `Path.Join` 的语义**（纯词法拼接是它的契约，且是热路径）
- [x] 1.2 `src/libraries/z42.core/tests/` 加 Normalize 单测（`a/./b` / `a//b` / `a/b/../c` / `./a` /
      `/` / `a/` / Windows 反斜杠混用 / 越过根的 `..`）
- [x] 1.3 `BuildLayout.Resolve` 对 `abs` 调 `Path.Normalize`

## 阶段 2：呈现（相对当前目录）
- [x] 2.1 `z42.project` 加 `BuildLayout.Display(string path)`：path 在 cwd 之下 → `./<相对段>`，
      否则返回规范化后的原路径（不生成 `../../..`，与 add-single-file-run 的诊断路径同口径）
- [x] 2.2 `src/compiler/z42c.driver/src/BuildCache.z42` 的 `cache -> ` 用 `Display`
- [x] 2.3 `src/toolchain/builder/core/builder_cli.z42` 的 `removed ` 用 `Display`
- [x] 2.3b **范围扩大（实施中发现）**：`wrote -> ` / `no changes; preserved -> ` 同属一族。
      默认工程它们碰巧相对（`DistDir` 走 `Path.Join(projectDir,"dist")` 分支），但工程一旦配
      `[build] output_dir`，`DistDir` 就变绝对 ⇒ 实测 `wrote -> /private/tmp/…/out/debug/dist/c.zpkg`
      与已修好的 `cache -> ./out/debug/.cache` 并排，比修之前更割裂。只修 2 处 = 症状级补丁
      （philosophy.md 根因修复 / 系统性修复），故一并接入 `Display`：
      `IndexedDist.z42` / `RuntimeConfigSidecar.z42` / `Main.z42`（4 处 `wrote ->` + 1 处 `preserved ->`）
- [x] 2.4 `src/libraries/z42.project/tests/build_layout.z42` 加 Display 用例

## 阶段 3：验证与文档
- [x] 3.1 `scripts/test/xtask_test_dist_cli.z42`：断言 `z42 build` / `z42 clean` 的输出**不含**绝对路径、
      不含 `/./`
- [x] 3.2 第 3 章 transcript 去掉 `[..]`，换成真实输出（`xtask test examples --bless` 后人工审 diff）
- [x] 3.3 完整 `xtask test` 全绿 + `xtask test dist`
- [x] 3.4 归档（PR 内完成）

## 备注

**GREEN（基于 main 0b4ba37d7）**：`xtask test` **13 stage 全绿**（4m58s）；`xtask test dist`
**657 通过 / 0 失败**（含新增 2 条路径断言）；`xtask test examples` 5 脚本 14 步全绿。

**修复后的实测输出**（默认工程 / 配 `output_dir` 的工程都验过）：

```
wrote -> ./out/debug/dist/d.zpkg (indexed, 1 zbc)
cache -> ./out/debug/.cache (1 files)
no changes; preserved -> ./out/debug/dist/d.zpkg
removed ./out/debug/dist
```

**两个过程教训**：
- 断言别钉死具体哪一行：最初写 `Stderr.Contains("cache -> ./artifacts/")`，而 dist 冒烟里该工程已被
  前一步 `z42 run` 建过 ⇒ 这次 build 走增量、打的是 `preserved -> …` 而非 `cache -> …` ⇒ 误报。
  改为断言**不变量**（无绝对路径 / 无 `/./` / 含 `-> ./`），两种形态都覆盖。
- **别在同一棵 worktree 里并发跑两轮 `xtask test`**：本次第一轮还在跑时就改了代码 + 重建 SDK + 起了
  第二轮，第一轮因产物被抽走报 `✗ z42c build failed`（假故障），两轮结果都不可信。清 `artifacts/.scratch`
  + 重建后单独跑一轮才作数。

- **自举种子（已核实）**：`z42.core` / `z42.project` 都是破环预建的 6 个库之一 ⇒ 新 API 可与使用点
  **同 commit**（bootstrap-seed 轴 ③ ⭐）。已 grep 确认 **xtask 源不使用 `BuildLayout`**，故不踩
  「xtask 最受约束」那条；z42b 在当前 stdlib 建好之后才编，也安全。
- ⚠️ `BuildLayout.ProjectDir` 文档写着「绝对路径」，规范化后仍是绝对路径，契约不变。
- ⚠️ 只改**呈现**，不改 `CacheDir` / `OutputDir` 的实际取值类型（仍是绝对路径）——z42c 要拿它建目录写文件，
  改成相对会让任何 chdir 破坏它。与 add-single-file-run 的「真实路径 vs 显示名分离」同一手法。
- **环境**：worktree `wt-paths`（基于 origin/main 0b4ba37d7）。供种后必须
  `rm -rf artifacts/build/{compiler,libraries}` 再冷建（种子 nightly 是 zpkg 0.43，main 已 0.47）。
  改 `scripts/` 下的源后必须 `./.z42/z42 publish scripts/xtask.z42.toml` 重建 xtask。
