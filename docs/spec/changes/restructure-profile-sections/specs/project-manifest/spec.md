# Spec: `[profile.*]` 的两个子表

> 新 capability `project-manifest` 的 delta。父提案 [proposal.md](../../proposal.md)。

## ADDED Requirements

### Requirement: `[profile.<n>]` 的内容是两个具名子表

#### Scenario: 运行时旋钮
- **WHEN** manifest 含 `[profile.release.runtime]` 且其中有标量键
- **THEN** 它们成为该 profile 的运行时旋钮，被烤进侧车的 `[runtime]` 段

#### Scenario: 应用属性
- **WHEN** manifest 含 `[profile.release.properties]`
- **THEN** 它原样成为该 profile 的应用属性，被烤进侧车的 `[properties]` 段

#### Scenario: 两个子表都可缺省
- **WHEN** 只写了其中一个（或都没写）
- **THEN** 另一个为空表，侧车相应地只出现有内容的那一段；两者皆空则不产侧车

#### Scenario: 边界由结构决定，不由清单决定
- **WHEN** 在 `[profile.release.runtime]` 里写任意键名
- **THEN** 它一律被当作运行时旋钮
- **AND** **NOT** 存在一份「哪些键名不算旋钮」的排除清单

---

### Requirement: `[profile.<n>]` 不接受直接写键

#### Scenario: 旧形状明确报错
- **WHEN** manifest 写 `[profile.release]` 后直接跟 `mode = "interp"`
- **THEN** 构建**失败**，消息指出运行时旋钮应写进 `[profile.release.runtime]`、
  应用配置应写进 `[profile.release.properties]`
- **AND** **NOT** 静默忽略该键

#### Scenario: 只有子表时正常
- **WHEN** `[profile.release]` 下只有 `[profile.release.runtime]` / `.properties`
- **THEN** 正常解析

---

### Requirement: profile 不再携带无人消费的构建期字段

#### Scenario: 字段已删除
- **WHEN** 检查 `Profile` 类型
- **THEN** **NOT** 存在 `Pack` / `Strip` / `Mode` / `Optimize` / `Debug` 字段
- **AND** 它们此前解析后全仓无任何消费方

## MODIFIED Requirements

### Requirement: 运行时旋钮在 manifest 中的位置

**Before:** `[profile.<n>]` 下的裸标量，靠 `pack/strip/optimize/debug` 排除清单与构建期
键区分。
**After:** `[profile.<n>.runtime]` 子表。裸标量一律报错。

## Pipeline Steps
- [x] manifest 模型与解析（`z42.project`）
- [x] 侧车生成（`z42c.driver`）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关

## IR Mapping
无。
