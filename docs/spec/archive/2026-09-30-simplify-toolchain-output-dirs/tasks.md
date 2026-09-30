# Tasks: toolchain 清单只配 output_dir，dist / publish 走级联默认

**状态：🟢 已完成 | 开始：2026-09-30 | 完成：2026-09-30（#960）**

类型：`refactor`（产物位置统一，不改构建行为）→ 最小化模式。

## 动机

toolchain 四个 apphost 组件的 `[build]` / `[platform.desktop]` 写法不统一：

| 组件 | dist | publish |
|---|---|---|
| launcher | 默认 `${output_dir}/dist` | 显式写，值 = 默认 |
| builder / devtools / interactive | `dist_dir = output_dir`（平铺） | 显式写，值 = 默认 |

平铺是历史遗留：zpkg 与 `.cache/`、`publish/` 挤在同一层，`build stage-toolchain` 还得专门跳过子目录。
`publish_dir` 四处都等于默认 `${output_dir}/publish`，纯冗余。

所有消费方都从清单解析路径，没有写死：xtask 的 `_toolchainZpkg` / `_toolchainDistDir` /
`_desktopPublishDir`，z42b 的 `_pubResolveZpkg` / `_pubDefaultPublishDir`。全仓搜 `build/toolchain/<组件>`
只命中注释。

## 进度概览

- [x] 阶段 1: 清单
  - [x] builder / devtools / interactive 删 `dist_dir`
  - [x] launcher / builder / devtools / interactive 删 `publish_dir`
  - [x] 同步清单注释
- [x] 阶段 2: 注释 / 文档
  - [x] `builder.z42` 开发树 entry-dir 注释（多了一层 `dist/`）
  - [x] `xtask_stdlib.z42` stage-toolchain 跳子目录的注释（原因已变）
  - [x] `artifacts-layout.md` toolchain 行
- [x] 阶段 3: 本地验证
  - [x] `xtask build toolchain`：四个组件均为 `toolchain/<组件>/{dist,.cache,publish}`，payload 完整性门通过
  - [x] 完整 GREEN（`xtask test`，12m04s，全阶段通过）
- [x] 阶段 4: PR CI（20 个检查全过）+ 归档

## 迁移提示

本地已有的构建树里，旧的平铺 zpkg（`artifacts/build/toolchain/{builder,devtools,interactive}/*.zpkg`）会残留。
它们不再被任何东西读取，删掉即可（或 `xtask clean all` 后重建）。

## 不在本 change

- 两个 workspace 的 `cache_dir = "${output_dir}/cache"` → 默认 `.cache`：会让所有库的缓存冷一次，另议。
- 目录级联在 z42.project `BuildLayout` / z42b / xtask 有多份实现：统一到 `BuildLayout` 是单独的 change。
