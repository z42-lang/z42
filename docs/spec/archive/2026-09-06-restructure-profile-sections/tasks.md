# Tasks: restructure-profile-sections

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/project-manifest/spec.md)

## P0 — 模型与解析
- [x] 0.1 `Profile`：删 `Pack`/`Strip`/`Mode`/`Optimize`/`Debug`；构造函数降为 `name + knobs + properties`
- [x] 0.2 `_parseProfiles`：`.runtime` → Knobs（复用 `_profileKnobs`，删 deny-list）、`.properties` → Properties
- [x] 0.3 `[profile.X]` 下的裸标量 → 明确错误，消息给出正确位置
- [x] 0.4 GREEN（stdlib 重编）

## P1 — 侧车生成器
- [x] 1.1 确认逻辑不变（读 Knobs / Properties，来源变了形状没变）
- [x] 1.2 自举不动点 gen1==gen2

## P2 — 迁移与文档
- [x] 2.1 6 个 manifest：`mode` 挪进 `.runtime`、删 `optimize = 2`
- [x] 2.2 `docs/book/src/runtime/runtime-settings.md` + `stdlib/app-properties.md` 的示例跟随
- [x] 2.3 GREEN 全套

## 未决
无。


## 落地记录（2026-09-05）

**一处设计自我更正**：design.md Decision 5 原写「检查放 loader」，实施时发现**仓内约定
相反**——`z42.project` 从不 throw，校验一律在消费方（如 `[[exe]]` 缺 entry 在 driver 报）。
改为：loader 把违规键搬进 `Profile.BadKeys`，driver 见非空即报错。已同步改 design。

**迁移面比提案估的宽**：提案说 6 个 manifest，实际 **10 个**——`src/` 下 7 个 +
`examples/` 4 个 + `scripts/xtask.z42.toml`。最后那个尤其容易漏：xtask 自己的 manifest
用旧形状，重编 xtask 时才报出来。

**踩到两个已知/新知的坑**：
1. `DepScanCache` 无 mtime 守卫（记忆里的 backlog）——改了被其它包依赖的 stdlib 包后，
   **首轮 `build stdlib` 假红、次轮成功**。此前只在格式 bump 时出现，现在日常开发也会咬。
2. 种子 `xtask.zpkg` 陈旧导致 `test lines` 按旧阈值（500）判，报 28 个假"新越界"。
   **第三次踩**——已写进记忆。

**手工端到端**：新形状 → 侧车两段正确 → 直跑读到 `mode=app-config` / `api=http://localhost`；
旧形状 → 明确报错并指出两个子表的位置。

**GREEN**：stdlib 319 文件；runtime cargo 1163/0；自举不动点 3/3；
e2e 568 + cross-zpkg 17 + multi-exe 2；launcher dist smoke 5/5；lines 6 known / 0 new-grown。
