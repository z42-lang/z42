# Design: `[profile.*]` 的两个子表

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/project-manifest/spec.md](specs/project-manifest/spec.md)

## Decisions

### Decision 1：边界靠结构，不靠清单

改前，「哪些键是运行时旋钮」= 「不在 `pack/strip/optimize/debug` 里的那些」——一份**手工
维护的排除清单**。改后 = 「在 `.runtime` 子表里的那些」——**结构**。

差别在失效模式：清单漏更新是**静默**的（新构建期键悄悄流进 app 侧车，用户每次跑都收到
一行 `unknown runtime knob`）；结构不可能漏——键要么在子表里要么不在。

`add-app-properties` 已经给属性做了这件事，本 change 只是把旋钮补齐，让两个子表与侧车
两段 1:1。

### Decision 2：`profile` 这个名字不改

Cargo 的 `[profile.dev]` / `[profile.release]` 表达的是「具名的构建配置」，z42 沿用是对的。
最初那个「要不要改名」的问题，真正的症结是**里面那层没有名字**——现在有了
（`.runtime` / `.properties`），名字问题自然消解。

### Decision 3：删掉 `Pack` / `Strip` / `Mode` / `Optimize` / `Debug`，不保留

五个字段解析后全仓无人读（`pm.Profiles` 的唯一消费方是侧车生成器，只碰 `Knobs` 与
`Properties`）。`pack` 的真实位置是 `[project]` 段；`mode` 自 `app-config-follows-the-app`
后走 `Knobs`。

**保留会更坏**：4 个工具链 manifest 里现在写着 `optimize = 2`，作者显然以为它生效。
把一个无人读的字段留在模型里，等于把这个误解固化成"看起来受支持"。

将来真要 profile 控制优化级别，那是一条独立的、需要接线到 z42c 优化管线的 change，
届时重新引入即可——**pre-1.0 不为可能的未来保留死代码**。

### Decision 4：`[profile.X]` 下的裸标量**报错**，不静默忽略

删掉那 5 个字段后，`[profile.X]` 自己不认任何标量。若静默忽略，正在迁移的人会遇到
「`mode = "interp"` 突然不生效、没有任何提示」——这是最坏的手感（与本项目在旋钮那边
坚持的「未知就明确告知」自相矛盾）。

故：遇到裸标量 → 构建期**错误**，消息直接给出该写的位置：

```
z42c build: [profile.release] 不接受直接写键（`mode`）——
  运行时旋钮写进 [profile.release.runtime]，应用配置写进 [profile.release.properties]。
```

选 error 而非 warning：这是**格式迁移**，写在旧位置的东西 100% 不会生效，warning 会被
淹没在构建输出里。

### Decision 5：检查放在 driver，不放 loader（实施时更正）

**起草时我写反了**：原以为「`[[exe]]` 缺 entry」的检查在解析侧，实施时查证**恰恰相反**
——`z42.project` 从不 throw，那条检查在 `z42c.driver/Main.z42` 里。仓内约定是
「z42.project 只搬运、不解释」，校验一律在消费方。

故改为：loader 把违规键原样搬进 `Profile.BadKeys`，**driver 见非空即报错**。
这既守住约定，又让报错发生在真正会因此产出错误侧车的那一方。

## Implementation Notes

- `Profile` 构造函数从 7 参降到 3 参（`name` + `knobs` + `properties`）——原来的两个
  重载合成一个。
- `_parseProfiles`：先扫顶层键，任何非表键 → 报错；`runtime` 子表走既有
  `_profileKnobs`（那个函数只收标量，现在收的是 `.runtime` 的标量，逻辑不变）；
  `properties` 子表原样保留。
- 侧车生成器不用改逻辑——它读的是 `Knobs` / `Properties`，来源变了、形状没变。
- 迁移的 6 个 manifest 都是机械改动；`optimize = 2` 直接删（无人读）。

## Testing Strategy

| 层 | 测试 |
|---|---|
| 解析 | `.runtime` 的标量进 `Knobs`；`.properties` 原样进 `Properties`；两者缺省为空 |
| 拒绝旧形状 | `[profile.X] mode = "interp"` → 明确错误，消息含正确位置 |
| 死字段已删 | `Profile` 上不再有 `Pack`/`Strip`/`Mode`/`Optimize`/`Debug` |
| 侧车 | 两个子表分别落到 `[runtime]` / `[properties]`；都空则不产文件 |
| e2e | 迁移后的 fixture（two_mains）经 `z42 run --bin` 仍对；自举不动点 |
