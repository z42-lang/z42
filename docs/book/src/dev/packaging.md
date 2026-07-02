# 打包引擎（packages.toml）

> **页型**: 机制页 ｜ **状态**: ✅ 已实现（desktop / ios / android / wasm）｜ **代码**: `scripts/package/` · `scripts/packages.toml`
> **相关**: [xtask](xtask.md) · 工具链·SDK 与发行包布局（待写）｜ **对齐**: 2026-07-02

## 概述

`package` 命令把仓库产物组装成发行包（SDK / runtime / workload）。核心是 `scripts/packages.toml`
数据清单：**产出与组装严格分层**——组件各自产出到暂存根 `artifacts/publish/<comp>/`，
包再从暂存根按 include 清单拷贝组装。加减包内组件只改一行 include，打包代码不动。

## 设计目标与约束

- **数据驱动**：包的内容是清单（TOML），不是代码里的硬编码拷贝序列
- **无隐形组件**：一切进包的东西都在组件注册表逐个登记（名字 → 产出方式 → 落点）
- **产出/组装解耦**：include 解析器只知道"去暂存根拷贝"，不知道"怎么产出"
- **跨平台同构**：四个 RID 类别（desktop / ios / android / wasm）共享同一清单机制

## 方案与决策

| 决策 | 选择 | 理由 |
|------|------|------|
| z42 程序类组件的产出 | 统一走 `z42 publish`（apphost kind） | 与用户发布自己 app 是**同一套机制**——z42c/launcher 不享特殊待遇，机制被日常 dogfood |
| 组件间依赖 | **不设依赖段**：组件独立产出；include 顺序只影响拷贝顺序 | 组件内部依赖（如 z42c 的六个兄弟包）由 `z42 publish` 自行解析拷贝，包层无需知道 |
| per-package 落点覆盖 | 不支持（dest 固定在组件注册表） | 当前无组件在两个包里需要不同落点；真需要再加，不预先设计 |
| runtime 包内容 | 仅 native + stdlib（不含 z42c / z42vm CLI） | runtime 包会跨 host 安装（如 android runtime 装在 macOS host），host 专属工具放进去无意义；自举种子由 SDK 包提供 |

## 机制

### 产出 → 组装两段流水

```mermaid
graph LR
    subgraph 产出 staging
        A[apphost 组件<br/>z42 publish] --> S[artifacts/publish/&lt;comp&gt;/]
        B[cargo-bin / cargo-native<br/>固定 handler] --> S
        C[stdlib-glob<br/>hard-link 全部 zpkg] --> S
    end
    S -->|按 package.include 拷贝<br/>{version}/{rid} 展开| P[artifacts/packages/<br/>z42-&lt;ver&gt;-&lt;rid&gt;-&lt;config&gt;/]
    P --> M[manifest 生成<br/>+ SHA-256 校验]
```

第一段各组件独立产出到暂存根（producer 互不依赖，可并行）；第二段按包定义的 include
清单逐组件拷贝、展开 `{version}` / `{rid}` 占位符、生成 manifest 与 SHA-256。

### 清单的三层结构

| 层 | TOML 段 | 内容 |
|----|---------|------|
| 包定义 | `[package.<name>]` | `artifact` 命名模板 + `include` 组件清单 + `manifest` 策略 |
| 组件注册表 | `[component.<name>]` | `kind`（产出方式）+ `project` + `dest`（包内落点） |
| 产出方式 | `kind` 枚举 | `apphost`（z42 publish）/ `cargo-bin` / `cargo-native` / `stdlib-glob` |

当前三个包：**sdk**（完整开发包：z42vm/native/stdlib/z42c/launcher/z42b/devtools/interactive）、
**runtime**（嵌入用：native + stdlib）、**workload-desktop**（仅 apphost-stub，per-RID 产出、
CI 合并四 RID）。包内布局的用户视角描述见工具链部分（待写）。

## 实现

| 组件 | 位置 | 要点 |
|------|------|------|
| 顶层分发（按 RID） | `scripts/package/xtask_package.z42` | desktop / ios / android / wasm 四管道 |
| 清单解析 | `scripts/package/xtask_packages_config.z42` | `[package.*]` + `[component.*]` 读取、include 名解析 |
| 固定 staging handler | `scripts/package/xtask_stage_components.z42` | z42vm / native / stdlib 三个产出函数 |
| desktop 管道 | `scripts/package/xtask_package_desktop.z42` | SDK 分段组装 + manifest + SHA-256 |
| 移动/浏览器管道 | `xtask_package_{ios,android,wasm}.z42` | native 产物 + 平台 facade（SwiftPM / Gradle / npm） |
| 自检 | `test packages-config / packages-staging / packages-assemble` | 解析、staging、组装三层各一个 harness |

## 边界与限制

- 组件落点全局唯一（无 per-package dest override）
- workload-desktop 单机只产 host RID，四 RID 合并发生在 CI（`release assemble-desktop-workload`）
- 发行包正确性验证依赖 `test dist`（需先打 host-RID 包）

## Deferred

- per-package dest override：同一组件需在不同包落不同位置时再引入
