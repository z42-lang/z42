# Spec: 包构建会话（BuildSession）—— z42b 路径

## ADDED Requirements

### Requirement: z42b 编译认清单的全部编译段

#### Scenario: [optimize] 生效
- **WHEN** 工程清单写 `[optimize] inline = false`，用 `z42b build --release` 构建
- **THEN** 产物与 `z42c build --release`（同清单）在内联决策上一致（不内联）

#### Scenario: [syntax] 生效
- **WHEN** 清单关掉某个语法特性，测试目标源码用到它，运行 `z42b test`
- **THEN** 编译失败并报 E0301（与 `z42c build` 同）

#### Scenario: [lints] / [analyzers] 生效
- **WHEN** 清单挂了 analyzer 并把其规则设为 error
- **THEN** `z42b test` 编译失败并显示该规则的诊断

#### Scenario: 版本号与 entry
- **WHEN** 清单 `version = "1.2.3"`、`entry = "App.Main"`
- **THEN** z42b 产物 META 的版本为 `1.2.3`，入口为 `App.Main`（此前恒 `0.0.0` / 自动探测）

### Requirement: 依赖声明与 z42c 同判

#### Scenario: 未声明依赖
- **WHEN** 源码 `using` 了一个未在 `[dependencies]` 声明、也不是自动可用 stdlib 的包的命名空间
- **THEN** z42b 编译报 E0497（此前 Z42_LIBS 下所有 zpkg 都被当成已声明）

#### Scenario: 声明的依赖找不到
- **WHEN** `[dependencies]` 声明的包在解析域里不存在
- **THEN** z42b 编译失败并指出缺的包名

### Requirement: 警告与调试符号不丢

#### Scenario: 警告可见
- **WHEN** 编译成功但有警告（如 W0700）
- **THEN** 警告经 z42b 输出呈现（此前成功时警告全部丢弃）

#### Scenario: release 保留 .zsym
- **WHEN** `z42b build --release`
- **THEN** 主 zpkg 旁有同名 `.zsym`（此前主包被剥符号而 `.zsym` 被丢弃）

## MODIFIED Requirements

**Before:** z42b 的 Compile 相位经 `Z42cCompiler` 独立实现，只认 `[sources]`。
**After:** 经 `BuildSession`（Role = HostTarget）——与 z42c 共享清单决议逻辑；`z42c build` 行为与产物字节不变。
