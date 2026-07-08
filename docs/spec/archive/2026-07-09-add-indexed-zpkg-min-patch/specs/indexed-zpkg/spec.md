# Spec: indexed zpkg（最小 patch 分发）

## ADDED Requirements

### Requirement: indexed 构建产物布局

#### Scenario: pack=false 构建
- **WHEN** `z42c build <toml>`（pack 决议 = indexed：显式 `pack=false` 或 debug 内置默认）
- **THEN** dist = `<name>.zpkg`（主文件：META/STRS/NSPC/EXPT/DEPS/SIGS/TSIG/IMPL + FILE 段）
  + 每源文件一个自包含 fullMode `<rel>.zbc`（子目录镜像）；不产 `.zsym`

#### Scenario: packed 行为不变（回归）
- **WHEN** pack 决议 = packed（显式 `pack=true` 或 release 默认）
- **THEN** 产物与本 change 前**逐字节相等**（packed 布局零变化；仅版本常量 bump）

#### Scenario: strip 冲突
- **WHEN** `pack=false ∧ strip=true`（toml 或 CLI override）
- **THEN** 构建报诊断错误退出（不静默忽略）

### Requirement: 最小 patch（User 核心裁定）

#### Scenario: 未变文件 zbc 逐字节不动
- **WHEN** indexed 工程 touch 1 个源文件后增量 `z42c build`
- **THEN** dist 中仅「失效闭包内文件的 `.zbc`」+ 主 zpkg 被重写；其余 `.zbc`
  **字节与 mtime 均不变** → patch = 主文件 + 变更 zbc 子集

#### Scenario: 增量与全量产物一致
- **WHEN** 同一源态下增量构建 vs `--no-incremental` 全量构建
- **THEN** dist 全部文件（主 + 散装）**逐文件字节相等**（对账器 indexed 腿断言）

#### Scenario: 删除源文件清理孤儿
- **WHEN** 删除源文件后构建
- **THEN** 主文件 FILE 目录同步 + 对应孤儿 `.zbc` 从 dist 清除

### Requirement: VM 装载

#### Scenario: indexed exe 直跑
- **WHEN** `z42vm dist/<name>.zpkg`（flags=indexed）
- **THEN** 按 FILE 目录装载全部散装 zbc，entry/ns/DEPS 取主文件，程序输出与 packed 等价

#### Scenario: zbc 内容错配
- **WHEN** 某 `.zbc` 与主文件 FILE 段记录的内容 hash 不符（散装文件被篡改/版本错配）
- **THEN** 装载报明确错误（指名文件），不静默降级

#### Scenario: strict-pin
- **WHEN** 主文件或散装 zbc 版本常量与 reader 不符
- **THEN** 拒绝装载（与 packed 同款 strict-pin，无兼容回退）

## IR Mapping
无新 IR 指令；zbc 格式**不动**（散装 = 既有 fullMode）；zpkg minor bump（FILE 段新布局 +
indexed 语义重定义，version-bumping 步骤 6-9 全套）。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker / IR Codegen：不变
- [ ] zpkg writer：`WriteIndexedMain`（FILE 段）+ pack 决议接通
- [ ] zpkg reader（z42c 侧）：Open 放行 indexed（DepScan 跨包消费）
- [ ] VM reader：indexed 装载（FILE 目录 + 逐 zbc fullMode 解码 + hash 校验）
