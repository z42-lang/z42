# Tasks: 0.4.0 按模块整合规划

> 状态：🟢 已完成 | 创建：2026-07-15 | 完成：2026-07-15
> **变更类型**：规划文档（docs）。本 change 只产出规划，不实施代码；各条目在启动时逐个开独立 spec。

## 进度概览
- [x] 阶段 1: 建 change 容器 + proposal
- [x] 阶段 2: design.md（6 模块整合表）
- [x] 阶段 3: spec.md（退出标准）
- [x] 阶段 4: User 裁决 Open Questions（Q1–Q5，2026-07-15 全定）
- [x] 阶段 5: 同步 roadmap.md 0.4.x/0.5.x/0.9.x 段（commit aeb7277b）
- [x] 阶段 6: 归档

## 阶段 1–3（已完成）
- [x] 1.1 建 `docs/spec/changes/replan-0.4.0-by-module/` 容器
- [x] 1.2 proposal.md：整合动机 + Scope + Open Questions（与 four-streams 冲突登记）
- [x] 2.1 design.md：6 模块 × 条目表（来源 todo#/FS-ID + 现状 + 依赖 + 锁协调）
- [x] 3.1 spec.md：各模块退出标准（可验证场景）
- [x] 3.2 versions.toml + src/runtime/Cargo.toml 版本 0.3.0 → 0.4.0（User 要求）

## 阶段 4: User 裁决（2026-07-15 全定）
- [x] 4.1 Q1 REPL → 上移 0.4.0（与 Playground 同批）
- [x] 4.2 Q2 runtime 组件化 + host 统一 → 上移 0.4.0（R8a host 统一先行 + R8b 组件化骨架）
- [x] 4.3 Q3 z42c 基础库入 stdlib → 沿用 `converge-z42c-onto-z42-project` 收敛范式，metadata/ir 各自出 spec
- [x] 4.4 Q4 tier2 平台测试 → 当前只全测 tier1，补齐 tier2（wasm/ios/android），定位为补齐
- [x] 4.5 Q5 G 流泛型前置 → 保留 0.4.0（G1 实例化 + G2 Invoke/MakeGenericType + G3 CreateInstance\<T\>）

## 阶段 5: roadmap 同步（裁决后）
- [x] 5.1 roadmap.md 0.4.x 段：模块视图指针 + 整合新增项表 + 退出标准扩展
- [x] 5.2 roadmap.md 0.5.x / 0.9.5 段：REPL(0.3.15) 划删、VM 组件化(0.9.5) 注上移、依赖链刷新
- [x] 5.3 依赖图刷新 + four-streams 死链修正（changes→archive 路径）

## 阶段 6: 归档
- [x] 6.1 tasks 状态 → 🟢；mv 到 archive/2026-07-15-replan-0.4.0-by-module
- [x] 6.2 commit（docs/spec/ 归档移动；版本 bump 已在 commit 3871372d 单独落）

## 备注
- 各条目落地时的 in-flight change 映射：R6=`optimize-zpkg-binary-layout`、R7=`inline-jit-safepoint-check`、S5=`add-partial-types`、T1=`wire-z42b-host-build`、T4=`add-workload-command-dispatch`、X3=`add-z42-wasm-playground`、C3=`split-irgen-class`、L4 相关=`converge-z42c-onto-z42-project`。
- 版本 bump 后续：commit → `git tag v0.4.0` → push 触发 release（**待 User 在正式发布时执行**，本 change 不 tag）。
