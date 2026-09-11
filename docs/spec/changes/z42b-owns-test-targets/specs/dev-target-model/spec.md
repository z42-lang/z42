# Spec: z42b 的测试/bench 目标模型

> Capability：`dev-target-model`。**dev target** = 一个包的测试或 bench 目标
> （显式 `[[test]]`/`[[bench]]`，或 `[tests]`/`[bench]` 段的 glob）。由 **z42b** 从真 manifest 解析、
> 编译、运行 —— 不再经 xtask 伪造的 mini-manifest。

## ADDED Requirements

### Requirement T1: 从真 manifest 解析目标

#### Scenario: glob 段
- **WHEN** manifest 有 `[tests] include = ["tests/**/*.z42"]`
- **THEN** 该 glob 命中的文件按**今天的分法**成为若干测试目标

#### Scenario: 显式目标
- **WHEN** manifest 有 `[[test]] name = "queue_tests" sources = ["tests/queue_*.z42"]`
- **THEN** 解析出一个名为 `queue_tests` 的目标，源为该 glob 命中集

#### Scenario: bench 同构
- **WHEN** `[[bench]]` / `[bench] include`
- **THEN** 解析规则与 test 完全一致（User：bench 与 test 同等处理）

#### Scenario: 三层依赖合并
- **WHEN** 目标被编译
- **THEN** 其依赖 = `[dependencies]` ∪ `[tests.dependencies]` ∪ `[[test]].dependencies`
- **AND** 后者优先级更高

### Requirement T2: 选择与默认

#### Scenario: 不指定 → 全部
- **WHEN** `z42b test`（或带 manifest 路径）
- **THEN** 编译并运行**全部** test 目标；任一失败 → 整体非零

#### Scenario: `--name` 选一个
- **WHEN** `z42b test --name queue_tests`
- **THEN** 只编译并运行该目标

#### Scenario: 名字无匹配
- **WHEN** `--name` 给了一个不存在的名字
- **THEN** **报错并列出可选目标名**，返回非零
- **AND NOT** 静默地跑零个目标然后返回 0

#### Scenario: 无任何目标
- **WHEN** manifest 既无 `[[test]]` 也无 `[tests]` 段
- **THEN** 报错说明未声明任何测试目标（非零）

### Requirement T3: 目标编译**不落合成文件**

#### Scenario: 内存派生
- **WHEN** 编译一个目标
- **THEN** 其 manifest 在**内存中派生**（换 name / sources / deps / kind / pack），**不写临时 toml**

#### Scenario: 产物隔离
- **WHEN** 同一包的两个目标先后编译
- **THEN** 各有独立的 `output_dir` / `cache_dir`，产物与缓存互不覆盖

#### Scenario: 强制 packed
- **WHEN** 在 debug profile 下编译 harness=true 目标
- **THEN** 产出 **packed** zpkg
- **BECAUSE** z42b 只能加载 packed zpkg；indexed + 散装 zbc 会被 runner 直接拒

### Requirement T4: 测试目标可见父包 `internal`

#### Scenario: internal 类与成员可用
- **WHEN** 父包有 `internal class Node`（及 `internal` 成员），目标源码里引用之
- **THEN** 编译通过（无 `E0404`）

#### Scenario: 只对父包放行
- **WHEN** 目标访问**其它依赖**（如 `z42.core`）的 internal
- **THEN** 照常 `E0404`

#### Scenario: `private` 仍不可见
- **WHEN** 目标访问父包某类的 `private` 成员
- **THEN** `E0404`（边界同 C# `InternalsVisibleTo`）

#### Scenario: 普通消费包不受影响（回归保护）
- **WHEN** 普通包 B 依赖 A 并访问 A 的 internal
- **THEN** 照常 `E0404`

### Requirement T5: `[Test]` / `[Benchmark]` 只能出现在测试目标里

#### Scenario: 目标里合法 / 普通包判错
- **WHEN** 测试目标源码里有 `[Test]` → 通过；普通包源码里有 → **E0457**
- **AND** 消息含被贴声明名 + 修复提示

#### Scenario: 无 manifest 豁免
- **WHEN** `z42c --emit-zbc <file> <out>`（golden 走这条）
- **THEN** **不报** —— 无包身份，且该路径不产生发布产物

### Requirement T6: `harness = false` 的 `entry` 可选

#### Scenario: 不写 entry
- **WHEN** `[[test]] harness = false` 且未写 `entry`
- **THEN** 由 `ZpkgBuilder.AutoDetectEntry` 自动探测（`.Main` / `Main` / `.main` / `main`）

#### Scenario: 歧义
- **WHEN** 自动探测返回 `"<ambiguous>"`（多个同级候选）
- **THEN** 报诊断，提示显式写 `entry`

#### Scenario: 显式覆盖
- **WHEN** 写了 `entry`
- **THEN** 以它为准（`RunTarget.HasEntry` 保留）

### Requirement T7: 零发现测试仍判红

#### Scenario: 目标编出来没有测试
- **WHEN** 某目标的产物里一条 `[Test]`/`[Benchmark]` 都没有
- **THEN** 判红（沿用已合的 #571），**不**静默返回 0

## MODIFIED Requirements

### Requirement: 测试单元的编译方式

**Before:** xtask 为每个单元**伪造一份 mini-manifest 落盘** → `z42c build <synth.toml>`。
「我是 X 的测试目标」这条身份在此丢失。

**After:** z42b 从**真 manifest** 解析目标 → **内存派生** → 同一个 `_orchestrate`。
父包身份全程保留，直达编译器。

### Requirement: `[[test]] harness = false` 的 `entry`

**Before:** 必填。 **After:** 可选（自动探测，歧义时报诊断）。

## Pipeline Steps

- [x] 工具链（z42b：目标解析 / 派生 / 运行）
- [x] 编译管线（`CompileInputs` 携带父包身份）
- [x] 符号导入（`IsImported` 判定）
- [x] 声明良构（E0457）
- [ ] Lexer / Parser / IR Codegen / VM —— **不涉及**

## IR Mapping

无。**不改 zbc/zpkg 格式、不改任何字节产物** —— internal 成员本来就在 zpkg 里
（导出/导入端均不按可见性过滤），本变更只改「允不允许用」与「谁来驱动编译」。
