# Spec: 0.4.0 退出标准（按模块）

> 0.4.0 是一条线（0.4.0–0.4.x），由本退出标准定义终点。每个模块的招牌项落地且验证通过即达标。

## ADDED Requirements

### Requirement: 编译器模块达标

#### Scenario: IR pass 首批上线
- **WHEN** 编译含常量表达式 / 死代码的源码
- **THEN** `IrPassManager` 常量折叠 + intrinsic 折叠 + DCE 生效，且 C# 与 z42c 双侧产出 byte-identical

#### Scenario: 增量 + 并发编译
- **WHEN** 二次 build 且部分 CU 未变更
- **THEN** 未变 CU 跳过重编；多 CU collect+typecheck 并行；ZbcWriter 产物确定性不变

### Requirement: 语法机制模块达标

#### Scenario: 五项小语法 golden 全绿
- **WHEN** 运行 `params` / `init`+表达式体属性 / 索引器 / 命名实参 / `partial` 的 golden
- **THEN** 全部通过 + 自举编译器源码 dogfood 实际用上

### Requirement: 标准库模块达标

#### Scenario: JSON serde 链完整
- **WHEN** 调用 `Deserialize<T>()` 反序列化任意用户类型
- **THEN** 自动绑定成功（依赖 G 流泛型实例化 + 泛型反射）

#### Scenario: z42c 基础库入 stdlib（若 Q3 裁决"做"）
- **WHEN** 用户代码引用抽象封装后的 metadata / ir 类型
- **THEN** 从 libraries 可用，且不破坏自举种子约束

### Requirement: runtime 模块达标

#### Scenario: JIT 直接 emit 拆箱证明收益
- **WHEN** 运行算术内循环 bench（已知 I64/F64）
- **THEN** 相比 helper 路径 2–3× 提速，且 bench 门禁防回退

#### Scenario: host 统一（若 Q2 裁决"做"）
- **WHEN** 在不同平台构建 VM 宿主
- **THEN** host/hostrun/main 走统一入口，平台差异收敛到共享抽象

### Requirement: 工具链模块达标

#### Scenario: z42b bench GA + 硬门禁
- **WHEN** PR 引入 >10% 性能退化
- **THEN** `z42b bench --diff` fail PR + 自动 diff 评论

#### Scenario: publish 不依赖 desktop
- **WHEN** 全新环境 `z42 publish`
- **THEN** 复用 build 流程产出，无需 desktop workload

### Requirement: 测试·产品·文档模块达标

#### Scenario: REPL 可用（若 Q1 裁决 0.4.0）
- **WHEN** 在 REPL 声明变量 / 表达式 / 类型并跨 line 引用
- **THEN** 正确求值 + 跨 line scope 保持

#### Scenario: 多平台测试流程绿
- **WHEN** CI 跑 WASM / iOS Simulator / Android Emulator
- **THEN** JUnit → GitHub Checks 全绿

## Out of Scope

- OSR/deopt 框架（留 0.5.x）；完整推测内联（R5 依赖 deopt）
- L3 大语法（`let` / 运算符重载 / `match`/ADT / async）
