# Tasks: compiler review 债务清理（refactor）

**变更说明：** docs/compiler_review.md 的低风险高确定性债务项批量清理。
**原因：** 死代码 + 单一 SoT 违反（drift 风险）+ 契约缺口；fixpoint 字节对账兜底，零语义变化。
**文档影响：** 归档时更新 compiler_review.md 对应项状态 + 目录 README（如文件增删）。

## 进度概览（每项独立 commit）
- [x] 1. Skeleton 死文件清理：6 个 *Skeleton.z42 自引用死簇（只互相 new、外部零引用）→ 删
- [x] 2. StrMap 去重：ir StrMapIr→StrMap（superset，多 TryAdd）；删 semantics/StrMap.z42；7 个 semantics 文件补 `using Z42.IR`；semantics 复用 ir（已依赖）
- [x] 3. Driver exit-code 契约：`ExitCode.{Ok=0/BuildError=1/UsageError=2}` 常量替 13 处魔数
  - 注：P2-4 的「命令分派 if 链→命令表」部分**未做**（当前 if 链小、Main.z42 待 converge 重构，低 ROI）——exit-code 契约是高价值部分

## 备注
- 子系统：compiler（单锁）。converge-z42c-onto-z42-project（DRAFT）re-queue。
- StrMap 落点：z42c.ir（semantics 已依赖 ir；无需新建 z42c.common）。
