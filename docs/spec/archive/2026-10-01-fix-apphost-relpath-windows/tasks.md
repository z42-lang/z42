# Tasks: Windows 上 `z42 publish` 产出的 apphost 嵌入路径错成 `../../C:/...`

**状态：🟢 已完成 | 开始：2026-10-01 | 完成：2026-10-01（#981）**

类型：`fix`（z42b，用户可见：Windows 桌面发布的 apphost 起不来）→ 最小化模式。

## 问题

#978（unify-build-layout-consumers）合入后，main CI 的 `package-host(windows-x64)` 在 desktop-publish smoke 红：

```
Error: cannot read `C:\...\ahsmoke\../../C:/Users/.../ahsmoke/app.zpkg`
    The filename, directory name, or volume label syntax is incorrect. (os error 123)
```

根因：`_pubRelPath`（apphost 嵌入「exe 目录 → zpkg」的相对路径）逐段比较、只按 `/` 切。#978 之后 zpkg 路径
取自 `BuildLayout.Resolve`，已 `Path.Normalize`（Windows 上 `C:/Users/...`）；exe 目录仍来自清单/命令行
（`C:\Users\...`）。两端分隔符不同 ⇒ 公共前缀为 0 ⇒ 相对路径把绝对路径整个拼了进去。#978 之前两端都是
未规范化的原始拼接，碰巧同形，所以没暴露。

## 方案

`_pubRelPath` 两端先 `Path.Normalize` 再比较 —— 在比较点消除分隔符差异，不依赖调用方传什么形式。
顺带删掉一句过时注释（「Mirrors the workload Apphost._relPath」—— 那份镜像已不存在）。

## 进度概览

- [x] `src/toolchain/builder/core/builder_apphost.z42`：`_pubRelPath` 两端规范化
- [x] 本地 GREEN（macOS，基底 b1fb49b2f，10m19s；本缺陷只在 Windows 显形）
- [x] PR CI：PR 上 package-host 被 path filter skip（z42b 不在 `platform`，由 #985 补上）⇒ 以合入后 main CI 为判据：`package-host(windows-x64)` dist test 816 passed / 0 failed（修前 814 / 1），desktop-publish smoke 绿；其后 main CI（6b0737fef）全绿
