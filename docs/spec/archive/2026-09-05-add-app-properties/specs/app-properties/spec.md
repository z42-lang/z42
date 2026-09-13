# Spec: 应用自定义配置属性

> 新 capability `app-properties`。父提案 [proposal.md](../../proposal.md)，方案 [design.md](../../design.md)。

## ADDED Requirements

### Requirement: manifest 用独立的 `[properties]` 表承载应用属性

#### Scenario: 基表
- **WHEN** manifest 顶层含 `[properties]` 表
- **THEN** 其内容成为所有 profile 共用的属性基表

#### Scenario: per-profile 覆盖
- **WHEN** 同时含 `[profile.<n>.properties]`
- **THEN** 该 profile 生效时，其顶层 key 逐个覆盖基表的同名 key；基表独有的 key 保留

#### Scenario: 浅覆盖
- **WHEN** 基表有 `limits = { a = 1, b = 2 }`，profile 有 `limits = { a = 9 }`
- **THEN** 生效值是 `{ a = 9 }`——**整体替换**，不深合并出 `{ a = 9, b = 2 }`

#### Scenario: 属性与旋钮互不干扰
- **WHEN** `[profile.<n>]` 有旋钮、`[profile.<n>.properties]` 有属性
- **THEN** 旋钮进侧车的 `[runtime]`、属性进 `[properties]`
- **AND** 属性里的键**NOT** 触发 "unknown runtime knob" 诊断
- **AND** `[runtime]` 里真正的未知键**仍然**触发该诊断

---

### Requirement: 侧车承载完整 TOML 类型的属性

#### Scenario: 标量 / 数组 / 嵌套表都能写出
- **WHEN** 属性含字符串、整数、布尔、数组、嵌套表
- **THEN** 生成的 `<app>.runtimeconfig.toml` 的 `[properties]` 段包含它们，且能被重新解析回等价结构

#### Scenario: 没有属性就没有该段
- **WHEN** manifest 既无 `[properties]` 也无 `[profile.<n>.properties]`
- **THEN** 侧车**NOT** 含 `[properties]` 段（与"无旋钮不产侧车"同一克制）

---

### Requirement: 运行时可读，只读，不分层

#### Scenario: 读顶层标量
- **WHEN** app 内调用 `AppProperties.Get("api-endpoint")`
- **THEN** 返回侧车 `[properties]` 里该键的值；整数 / 布尔渲染为字符串

#### Scenario: 读结构化值
- **WHEN** 调用 `AppProperties.Raw()` 并交给 `Std.Toml` 解析
- **THEN** 得到与 manifest 中等价的完整结构（数组 / 嵌套表可正常访问）

#### Scenario: 未知 key
- **WHEN** 调用 `Get` / `Has` 一个不存在的键
- **THEN** 分别返回 `null` / `false`——这是**正常情形**，不产生诊断

#### Scenario: 枚举
- **WHEN** 调用 `AppProperties.Names()`
- **THEN** 返回全部顶层键

#### Scenario: 不参与分层
- **WHEN** 用户配置（`Z42_CONFIG`）里写了 `[properties]`
- **THEN** 它**不**生效
- **AND** 产生一行 warn 说明属性归 app 所有
- **AND** **NOT** 静默忽略

#### Scenario: 不能从 CLI 设置
- **WHEN** 尝试 `--set` 一个属性名
- **THEN** 按未知旋钮处理（报错 + 最近邻建议）——属性不是旋钮

#### Scenario: 只读
- **WHEN** 检查 `Std.Runtime.AppProperties` 的公开成员
- **THEN** **NOT** 存在任何写入 / 修改方法

#### Scenario: 所有运行形态一致
- **WHEN** 经 `z42vm <app>` 直跑 / `z42 run` / 已发布 apphost / 嵌入入口运行
- **THEN** 属性都读得到（沿用侧车既有的到达路径）

## Pipeline Steps
- [x] manifest 模型与解析（`z42.project`）
- [x] 侧车生成（`z42c.driver`）
- [x] VM 读取 + corelib builtin
- [x] stdlib 表面（`Std.Runtime.AppProperties`）
- [ ] Lexer / Parser / TypeChecker / IR Codegen — 无关

## IR Mapping
无（不新增 IR 指令 / 不改 zbc·zpkg 格式；属性走侧车文件）。
