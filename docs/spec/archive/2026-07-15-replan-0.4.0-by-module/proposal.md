# Proposal: 0.4.0 按模块整合规划（todo-list.md ⋈ four-streams）

> 状态：🔴 DRAFT，待 User 确认整合方向（尤其 Open Questions 中与已裁决 four-streams 的冲突项）。

## Why

0.3.x 自举线收尾在即（z42c 已 byte-identical 编译全 stdlib）。0.4.x 现有两份来源，视角与范围**不一致**：

1. **已归档 four-streams 规划**（`archive/2026-06-23-plan-0.4.x-four-streams/`，经 User 裁决）：按 **P/B/S/L/G 并行流**组织，主题=「质量与性能兑现」。
2. **User 的 `docs/todo-list.md` 第 11 行**（0.4.0 心愿单）：范围更广，含 **REPL / Playground / runtime 组件化 + host 统一 / z42c 基础库入 stdlib / 多平台测试流程 / book 整理**——其中数项在 four-streams 中位于其他版本（REPL@0.3.15、VM 组件化@0.9.5、CI 三平台@0.3.13）或全新。

两者不整合的代价：0.4.0 实际范围（User 心愿单）与文档 SoT（four-streams）脱节，实施时无权威依据、反复返工。

## What Changes

- **以 todo-list.md 第 11 行为 0.4.0 权威范围**，four-streams 的 P/B/S/L/G 作为已设计好的实现细节回填。
- **产出一份按模块组织的 0.4.0 规划**（design.md），6 个模块：`编译器` / `语法机制` / `标准库` / `runtime` / `工具链` / `测试·产品·文档`，每项标注：来源（todo# / four-streams ID）、现状（已落地 / in-flight change / 待做）、依赖。
- **登记与 four-streams 的差异裁决**（见 Open Questions）：REPL 上移、VM 组件化上移、z42c-lib 入 stdlib 新增等，需 User 明确"以 todo 为准"。
- **本变更不改 roadmap.md**：roadmap 是长期 SoT，待整合方向经 User 确认后，另开 commit 同步 0.4.x 段（避免在未裁决前污染 SoT）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/spec/changes/replan-0.4.0-by-module/proposal.md` | NEW | 本提案 |
| `docs/spec/changes/replan-0.4.0-by-module/design.md` | NEW | 6 模块 × 条目整合表（含来源/现状/依赖）|
| `docs/spec/changes/replan-0.4.0-by-module/specs/roadmap-0.4.0/spec.md` | NEW | 0.4.0 退出标准（可验证场景）|
| `docs/spec/changes/replan-0.4.0-by-module/tasks.md` | NEW | 落盘任务清单 |

**只读引用**：

- `docs/todo-list.md` — 0.4.0 权威范围来源（第 11 行）
- `docs/spec/archive/2026-06-23-plan-0.4.x-four-streams/{proposal,design}.md` — 实现细节回填
- `docs/roadmap.md` — 0.4.x/0.5.x/0.9.x 现状（冲突比对）
- `docs/spec/changes/` 下 in-flight change 目录 — 现状标注

## Out of Scope

- **不实施任何代码**：本变更只产出 0.4.0 规划文档；各条目 spec 在启动时逐个开。
- **不改 roadmap.md**：待 Open Questions 裁决后另开 commit。
- **不动 0.3.x 收尾 in-flight**：split-irgen-class / converge-z42c-onto-z42-project 等按各自节奏。

## Open Questions（与已裁决 four-streams 的冲突，需 User 裁决）

> **2026-07-15 User 全部裁决，见下。**

- [x] **Q1 REPL**：**上移到 0.4.0**（与 Playground 同批作产品能力）。原 0.3.15 capstone 归属废止。
- [x] **Q2 runtime 组件化 + host/hostrun/main 统一**：**确认上移到 0.4.0**。拆两半：R8a 先做 host/hostrun/main 统一（不同平台共享简化），R8b 组件化先铺 cargo-feature 骨架；原 0.9.5 归属废止。
- [x] **Q3 z42c 基础库入 libraries**：**沿用已有收敛范式**——in-flight `converge-z42c-onto-z42-project` 已确立「z42c 自用库 → 共享 stdlib」模式（project model → `z42.project`，zpkg 后端拆 `z42c.zpkg`）。metadata / ir 是该范式下一批候选，落地前各自出设计 spec 定边界（勿破坏自举种子约束）。
- [x] **Q4 多平台测试流程**：**当前 CI 只全测 tier1（linux/macos/windows）**；0.4.0 补齐 **tier2（wasm / ios / android）** 测试流程（对应 four-streams「CI 三平台模拟器」）。定位为**补齐**而非重做。
- [x] **Q5 G 流泛型前置**：**保留在 0.4.0**，作 JSON `Deserialize<T>` serde 前置（G1 运行期泛型实例化 + G2 泛型方法 Invoke/`MakeGenericType` + G3 `Activator.CreateInstance<T>`）。
