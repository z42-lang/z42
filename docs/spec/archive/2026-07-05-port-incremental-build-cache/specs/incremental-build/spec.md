# Spec: z42c 增量编译 cache（packed 模式）

## ADDED Requirements

### Requirement: 单文件 .zbc cache 落盘

#### Scenario: 默认 cache 目录
- **WHEN** `z42c build <toml> --release` 且 toml 设 `[build].output_dir`、未设 `cache_dir`
- **THEN** 每个参编 `.z42` 在 `${output_dir}/.cache/<rel>.zbc` 产出 fullMode zbc，build 成功产物不变

#### Scenario: 显式 cache_dir（含模板）
- **WHEN** toml 或 workspace `[workspace.build]` 设 `cache_dir = "${output_dir}/cache"`
- **THEN** cache 落该目录（模板解析同 dist_dir 级联）

#### Scenario: 无 [build] 工程
- **WHEN** toml 无 `[build]` 段
- **THEN** cache 落 `<projectDir>/.cache/`（与历史 z42c 默认 `<projectDir>/dist` 同基准）

### Requirement: 增量 probe（any-fresh→all-fresh；全命中→跳过重写）

#### Scenario: 全命中跳过
- **WHEN** 上次 build 后未改任何源文件，再次 `z42c build`
- **THEN** 日志 `cached: N/N files` + 跳过说明，**不重编不重写**——dist 内 zpkg 保持原字节；
  exe 的依赖 zpkg 复制仍执行

#### Scenario: 任一文件改动 → 整包重编
- **WHEN** 修改任意 1 个源文件后 `z42c build`
- **THEN** 整包全量重编（`cached: 0/N`），产物与 `--no-incremental` build **逐字节相等**

#### Scenario: 源文件增删 → 全 fresh
- **WHEN** 新增或删除源文件后 `z42c build`
- **THEN** 记录数不符 / no-record → 全量重编（不得保留含已删模块的旧 zpkg）

#### Scenario: cache 缺失/损坏安全回退
- **WHEN** cache 目录被删或上次 zpkg 不可读
- **THEN** 全量重编，build 正常完成、不报错

#### Scenario: 强制全量
- **WHEN** `z42c build --no-incremental`
- **THEN** 跳过 probe，行为与现状 MVP 相同（含重写 zpkg + cache）

#### Scenario: miss 诊断
- **WHEN** `Z42_INCR_DEBUG=1` 且存在 miss
- **THEN** 逐文件输出 miss 原因（no-record / hash-diff / no-zbc / no-export-mod）

#### Scenario: workspace 模式不受影响
- **WHEN** `z42c build --workspace`（或显式 `--output-dir`）
- **THEN** 不落 cache、不 probe，行为与本 change 前逐字节一致（gen1==gen2 门禁不变）

## IR Mapping
无新 IR 指令 / 无 zbc·zpkg 格式变更（fullMode zbc 与 packed zpkg 均既有格式，版本号不动）。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen：不变（跳过或全量两态）
- [ ] zbc writer：复用 ZbcWriter（新增 cache 落盘调用点）
- [ ] zpkg reader：新增 MODS 头消费面 `ReadSourceHashes`（格式不变）
- [ ] VM interp：零改动
