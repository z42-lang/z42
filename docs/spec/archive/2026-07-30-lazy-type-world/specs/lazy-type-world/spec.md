# Spec: 惰性化跨包类型世界

## ADDED Requirements

### Requirement: 首次 eval 成本 = O(引用闭包)，不随标准库增大恶化

`ScanDirsLazy` 不再一次性全量解析 world 的 TYPE/SIGS；类型世界 `Wp` 按包懒填，基类链祖先按命名空间
路由按需解析。

#### Scenario: 无外部引用 → 不解析任何额外包
- **WHEN** REPL 首次 eval `1 + 2`
- **THEN** 除 prelude（+其基类链闭包）外，world 不解析任何额外包的 TYPE/SIGS

#### Scenario: 引用一个包 → 只解析其基类链闭包
- **WHEN** 首次 eval `Console.WriteLine("x")`（引用 `Std.IO`）
- **THEN** 只解析 `z42.io` + 其基类链祖先包（实测闭包 ≤3），而非全部 ~34 个包

#### Scenario: 库变多不拖慢
- **WHEN** 标准库包数从 N 增到 2N（新增包不进已有类的基类链）
- **THEN** 引用某包的首次 eval 的类型解析成本不变（仍 = 该包的闭包，与总包数无关）

### Requirement: 基类链解析正确性与产物逐字节不变

懒填 world 产出的 `ExportedModuleZ` / `DependencyIndex` 与 eager 全量 world 逐字节相同。

#### Scenario: 跨包基类链解析不变
- **WHEN** 类 `class Sub : Base@其他包`（导入基类）经懒填 world 重建
- **THEN** 继承字段 / 虚派发 / 上转型 / 方法合并（topmost-first、override 保留祖先位）与 eager world 完全一致

#### Scenario: 自举字节不动点
- **WHEN** 用惰性 world 的 z42c 自举编译 z42c 源码（gen1 → gen2）
- **THEN** gen1 == gen2 逐字节相同（编译器自身大量跨包基类链，最强回归证据）

#### Scenario: 命名空间跨多包
- **WHEN** 基类 FQ 的命名空间由多个包共同声明，而基类只在其中一个包
- **THEN** 路由解析**所有**声明该 ns 的包，正确定位到基类所在包（与全量扫描等价）

### Requirement: 零格式 / 零 VM 变更

#### Scenario: 格式不 bump
- **WHEN** 本 change 落地
- **THEN** zbc / zpkg 的 major/minor 版本不变；已有 zpkg 产物无需重生；VM 零改动

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。纯 z42c 前端跨包类型解析机制（DepScan/TsigReconcile）的惰性化重构。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker — 无
- [x] 跨包依赖解析（DepScan / TsigReconcile）— 惰性 world
- [ ] VM interp / 格式 — 无
