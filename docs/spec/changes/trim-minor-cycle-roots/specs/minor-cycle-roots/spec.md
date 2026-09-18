# Spec: 周期中的 minor 根集

## MODIFIED Requirements

### Requirement: 增量 major 周期打开期间，minor 的额外根

**Before:** 灰队列（`mark_queue`）与 SATB 队列（`satb_queue`）的**全部**条目都是该 minor 的根。
**After:** 只有其中**年轻**（`gen_age < promotion_age`）的条目是根。老条目不入根——
minor 不回收它们，其年轻子节点由卡表覆盖（与 minor BFS「不把老年子节点入队」依赖同一条不变量）。

#### Scenario: 老灰条目的年轻子节点仍然存活
- **WHEN** 一个增量 major 周期打开，灰队列里有一个**老**对象 X，X 有一个**年轻**子节点 Y，其间发生一次 minor
- **THEN** Y 不被回收（X 的卡是脏的，`seed_from_dirty_cards` 会追到 Y）；周期结束后 X 与 Y 都存活

#### Scenario: 年轻灰条目仍然不被回收
- **WHEN** 灰队列里有一个**年轻**对象 G，且 G 已不被任何 mutator 根可达，其间发生一次 minor
- **THEN** G 不被回收（它仍是根），标记器随后能安全地追它——这正是 M2a 那条规则保护的形状

#### Scenario: 阴性对照
- **WHEN** 把年轻灰条目也一并跳过（即整条规则失效）
- **THEN** 上一条场景中的 G 被回收 —— 测试必须失败，用以证明该断言真的在守东西

#### Scenario: 周期期间的回收结果不变
- **WHEN** 在一个开着周期的负载上完整跑完
- **THEN** 程序输出与改动前一致；编译器自举负载的产物逐字节一致

### Requirement: 诊断可见性

#### Scenario: `Z42_GC_PHASES` 打开
- **WHEN** 设置 `Z42_GC_PHASES=1`
- **THEN** 每次 minor 的 `minor roots` 行分别报告**播种的**与**因年龄跳过的**灰/SATB 条目数

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM runtime（GC：minor 根集）
