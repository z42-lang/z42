# Tasks: G0 泛型反射规划

> 状态：🔴 DRAFT，待 User 确认规划 + Open Questions | 创建：2026-07-20
> **G0 = 规划**（docs-only，本地环境受限下正合适）；G1–G3 各自开 change。

## 进度概览
- [x] 阶段 1: 勘察泛型模型 + 三件套复用/缺口
- [x] 阶段 2: G0 规划文档（proposal + design + spec）
- [ ] 阶段 3: User 裁决 Open Questions（Q1–Q4）
- [ ] 阶段 4: 落 roadmap G 流细化 + 归档 G0

## 阶段 1–2（已完成）
- [x] 1.1 勘察：具化擦除模型 + make_constructed_type + ObjNew type_args + activator_create
- [x] 1.2 关键发现：runtime instantiation 无需 codegen（擦除红利）→ G 流量级下修
- [x] 2.1 design.md：三件套 → 复用/缺口/落点 + 分阶段 G1–G3 路线
- [x] 2.2 spec.md：G1–G3 目标行为场景

## 阶段 3: User 裁决（2026-07-21 全定）
- [x] 3.1 Q1：复用 GetGenericTypeDefinition 定义型句柄（G1 期核对充分性）
- [x] 3.2 Q2：泛型方法 type_args 供给后置 G2
- [x] 3.3 **Q3：MakeGenericType/泛型 CreateInstance 必须运行期校验约束**（User：语言要安全）——上调为 G1 必做，复用 IsAssignableFrom + 约束元数据
- [x] 3.4 Q4：交付顺序 G1→G2→G3 认可

## 阶段 4: 落盘 + 归档
- [ ] 4.1 roadmap.md：0.4.x G 流条目细化（量级重估"轻，无 codegen" + G1–G3 拆分）
- [ ] 4.2 reflection.md Deferred：泛型反射路线指针
- [ ] 4.3 归档 G0

## 备注
- 环境：并发 WIP 占主 checkout + stale 0.32 种子——G0 纯文档不受影响；G1 起的 runtime 实现需 worktree + CI-PR 循环（同接口成员枚举）。
- G1 是纯 runtime、喂 serde 基本盘的最小前置——Q1/Q4 定了即可开工。
