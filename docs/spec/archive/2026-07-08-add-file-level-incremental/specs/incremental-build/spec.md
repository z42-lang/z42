# Spec: 文件级增量编译（packed 模式）

## MODIFIED Requirements

### Requirement: 增量粒度（原：整包 any-fresh→all-fresh）

**Before:** 任一源文件变化 → 整包全量重编；判定依据 = 上次 zpkg MODS。
**After:** 仅重编「变化文件 + 包内传递依赖方」；判定与组装依据 = cache（zbc + meta）。

#### Scenario: 改动叶子文件（无包内依赖方）
- **WHEN** 包内仅文件 X 变化，且没有其他文件引用 X 定义的符号
- **THEN** 仅 X 重编（`cached: N-1/N`），dist zpkg 整包重写，产物与 `--no-incremental`
  全量 build **逐字节相等**

#### Scenario: 改动被依赖文件（传递失效）
- **WHEN** 文件 B 变化，A 引用 B 定义的符号（A→B），C 引用 A（C→A）
- **THEN** 失效集 = {B, A, C} 全部重编，其余文件从 cache 重建；产物与全量**逐字节相等**

#### Scenario: 全命中跳过（沿用）
- **WHEN** 无任何源文件变化
- **THEN** 不重编不重写，dist zpkg 原字节保留（`cached: N/N` + preserved）

#### Scenario: 源文件增删
- **WHEN** 新增或删除源文件
- **THEN** cache 集与 srcs 集不一致 → 全量重编（不得保留陈旧模块）

#### Scenario: cache 损坏 / 版本不符
- **WHEN** meta 缺失、zbc 损坏、或 meta 版本 pin 与当前 writer 不符
- **THEN** 对应条目（版本不符则整 cache）作废 → 相应文件按 fresh 处理，build 正常完成

#### Scenario: 强制全量
- **WHEN** `z42c build --no-incremental`
- **THEN** 跳过 probe，全文件重编 + 重写 cache 与 dist

#### Scenario: workspace 模式不受影响（沿用）
- **WHEN** `z42c build --workspace` / 显式 `--output-dir`
- **THEN** 不落 cache、不 probe，行为与现状逐字节一致（gen1==gen2 门禁不变）

### Requirement: dist 为 cache 的确定性投影

#### Scenario: packed 整包重写
- **WHEN** 任一 cache 条目变更（增量重编产生新 zbc/meta）
- **THEN** packed zpkg 从 cache 条目全集按源序组装并整包重写

#### Scenario: 确定性
- **WHEN** 同一源集以任意「touch 序列 + 增量/全量组合」到达同一最终源状态
- **THEN** dist zpkg 与该源状态的一次全量 build **逐字节相等**（暴力对账器逐文件验证）

## ADDED Requirements

### Requirement: 暴力对账器

#### Scenario: 语料级逐文件对账
- **WHEN** 运行 `xtask test incremental`
- **THEN** 对语料（z42c 7 包 + 代表 stdlib 包 + launcher）逐文件 touch，断言增量产物与
  全量产物逐字节相等；任一不等即失败并指出文件

## IR Mapping
无新 IR 指令 / 零 zbc·zpkg wire 格式变更（cache meta 为 z42c 内部文件，自带版本 pin）。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen：不变（作用于失效子集）
- [ ] zbc writer：不变（cache 落盘沿用）
- [ ] **zbc reader（新增消费方）**：ZbcReader.Read → IrModule（cached 文件重建）
- [ ] zpkg 组装：输入切换为 cache 条目全集
- [ ] VM interp：零改动
