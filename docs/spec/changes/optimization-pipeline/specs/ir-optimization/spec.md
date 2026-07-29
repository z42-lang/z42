# Spec: 编译期 IR 优化管线（temp-DCE 首发）

## ADDED Requirements

### Requirement: 编译期 IR 优化管线框架

z42c 在 emit IrModule 后、写 zbc 前跑一个引擎无关的 IR 优化管线（`IrOptPipeline.Run`），逐函数应用
可组合的 pass。pass 只用 z42.ir 现有 public 字段（compiler 源码内 type-switch），不新增 z42.ir API。

#### Scenario: 管线遍历所有函数
- **WHEN** IrGen.Generate 产出含 N 个函数（含类方法——均在 `IrModule.Functions`）的模块
- **THEN** IrOptPipeline.Run 对每个函数运行 pass；抽象/extern 桩（BlockCount==0）跳过

#### Scenario: 加新 pass 不改流程主体
- **WHEN** 后续新增 copy-prop / const-fold
- **THEN** 作为新方法插入 `_optFunc`，`Run`/遍历/读计数框架不变

### Requirement: temp-DCE 删除死的纯指令

一条指令若「在可删白名单内（纯：不抛/不调用户码/不写内存/不分配）」且「其 Dst 寄存器全函数零读」，
则删除；否则保留。未知 opcode 一律保留（安全默认）。

#### Scenario: 死的纯计算被删
- **WHEN** 函数含 `dead = n*n + 7` 且 `dead` 从不被读
- **THEN** 写 `dead` 的纯指令被删；输出不变；interp dispatch 更少

#### Scenario: 有副作用指令即使 Dst 未读也保留
- **WHEN** `Call foo()` 结果寄存器从不被读
- **THEN** Call 保留（副作用：foo 仍须执行）

#### Scenario: 会陷阱的指令保留
- **WHEN** `Div`/`Rem`/`FieldGet`/`ArrayGet` 的 Dst 未读
- **THEN** 保留（除零/NPE/越界陷阱是可观测行为，删除会改变语义）

### Requirement: 参数寄存器 live-out（正确性不变量）

DCE 判活必须把参数寄存器视为 live-out——out/ref 参数的最终值由调用方读取，函数内 read-count 看不到。

#### Scenario: out 参数写入被保留（out_var 回归）
- **WHEN** `bool TryParse(string s, out int v) { v = 42; return true; }`，`v` 在函数内不再被读
- **THEN** 写 `v = 42` 的指令**不被删**；调用方 `Assert.Equal(42, n)` 读到 42（非 Null）

#### Scenario: 语义与自举不变
- **WHEN** 管线在全 stdlib + z42c 源码上运行
- **THEN** e2e goldens 全绿（interp+jit 输出不变）；z42c 自举 gen1==gen2 逐字节复现

## IR Mapping
无新 IR 指令/opcode/zbc 格式变更——本变更**减少** emit 的指令条数，格式不变。

## Pipeline Steps
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [x] IR Codegen — IrGen.Generate 末尾挂 IrOptPipeline.Run（新 IrOptInfo / IrOptPipeline）
- [x] VM interp — 无改动，受益于更精简 IR
