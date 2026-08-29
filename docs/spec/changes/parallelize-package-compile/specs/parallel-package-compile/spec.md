# Spec: 包编译文件级并行

## ADDED Requirements

### Requirement: ParallelFor 工作池确定性

#### Scenario: 并行结果与串行一致
- **WHEN** `ParallelFor(n, jobs, body)` 每个 `body.Run(i)` 写独立输出槽 `out[i]`
- **THEN** 对任意 `jobs>=1`，`out[]` 的内容与 `for i in [0,n): body.Run(i)` 串行执行**完全一致**（完成顺序不影响结果）

#### Scenario: jobs<=1 退化串行内联
- **WHEN** `--jobs 1`（或未指定且核数为 1）
- **THEN** 不起任何线程，`body.Run` 在主线程内联顺序执行——作为逃生舱与调试基线

#### Scenario: worker 异常不被吞
- **WHEN** 某 `body.Run(i)` 抛异常
- **THEN** 该异常被记录、`Join` 后由主线程聚合汇报/重抛，不静默丢失；其它已完成项的结果不受影响

#### Scenario: 边界
- **WHEN** `n == 0`
- **THEN** 直接返回，不起线程
- **WHEN** `n < jobs`
- **THEN** 实际线程数取 `min(jobs, n)`

### Requirement: 并行编译产物与串行逐字节一致

#### Scenario: --jobs 不影响产物字节
- **WHEN** 同一包（含 type-alias 与 where 约束的多文件）分别 `z42c build --jobs 1` 与 `--jobs 8`
- **THEN** 产出的 `.zpkg`（及 indexed 模式各 `.zbc`）**逐字节一致**

#### Scenario: 自举不动点在并行下成立
- **WHEN** `z42c build --workspace` 并行编译 z42c 自身两遍（gen1、gen2）
- **THEN** gen2 与 gen1 的各成员 zpkg **section 级一致（忽略 BLID）**——并行不破坏自举不动点

### Requirement: 并行段前共享可变状态已冻结

#### Scenario: per-file 别名隔离
- **WHEN** 并行 per-file 编译，文件 A 与文件 B 有不同的 `using X = Y;` 别名
- **THEN** 各文件经 `SymbolTable.WithAliases` 持独立别名视图，A 的类型解析绝不读到 B 的别名（无数据竞争、无串味）

#### Scenario: 约束解析不在并行段写共享表
- **WHEN** 包内有带 `where` 约束的泛型类，并行编译其文件
- **THEN** `ClassConstraints` 的写在并行段**之前**完成（前置或 hoist），并行段内对其只读

### Requirement: z42c 并行依赖的自举种子

#### Scenario: warm 构建
- **WHEN** in-tree 已有 z42.threading 产物，`xtask build compiler`
- **THEN** z42c 正常自建并运行期加载 z42.threading，并行编译可用

#### Scenario: cold 构建
- **WHEN** fresh checkout 无 in-tree 种子，冷启动 `build compiler`
- **THEN** 种子解析供给 z42.threading（上一 nightly libs 或 `_ensureBootstrapSelfDepLibs` 预建），z42c 建成且运行期能加载它——不报 `undefined function`/缺库

## Pipeline Steps

受影响的 pipeline 阶段：
- [x] 源读取 / SHA-256（driver `_build`）——★① 并行
- [x] per-file TypeChecker + IrGen codegen（`IrDump.BuildPackageCus` / `CuCompile._compileCu`）——★② 并行
- [x] 组装 / dist 落盘 I/O——★③ 并行
- [ ] Lexer / Parser——不变（parse-all 已在段前，本变更不动其并行）
- [ ] DepScan / CollectAll——不变（串行 setup）

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更——纯编排 + 并发，产物字节不变（这正是确定性验收的核心）。
