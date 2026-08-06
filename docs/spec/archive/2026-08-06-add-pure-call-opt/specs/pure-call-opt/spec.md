# Spec: 纯函数调用优化

## ADDED Requirements

### Requirement: 纯度推断（模块不动点）

#### Scenario: 纯标量函数判纯
- **WHEN** `int sq(int x) { return x * x; }`
- **THEN** `IrPureFunctionTable` 判 `sq` 为纯

#### Scenario: 读 readonly 字段仍纯
- **WHEN** `class C { readonly int f; int scale(int k){ return k * this.f; } }`（f 只在 ctor 赋值）
- **THEN** `scale` 判纯（readonly 字段读确定）

#### Scenario: 读非 readonly 字段不纯
- **WHEN** 同上但 `f` **非** readonly
- **THEN** `scale` 判**非**纯（字段可变、两次调用可能不同值）

#### Scenario: 写/IO/分配/抛/调非纯 → 非纯
- **WHEN** 函数体含 `field_set` / `Console.WriteLine` / `new T()` / `throw` / 调用非纯函数 / `Div`
- **THEN** 判**非**纯

#### Scenario: 递归纯函数
- **WHEN** `int f(int n){ return n <= 0 ? 0 : f(n-1); }`（只算术 + 自调 + 分支）
- **THEN** 判纯（不动点乐观初值收敛）

#### Scenario: imported / 无体函数保守非纯
- **WHEN** 调用跨 zpkg 函数或 abstract/extern 无体桩
- **THEN** 调用方就该点判**非**纯（找不到摘要 → 保守）

### Requirement: 纯调用块内 CSE

#### Scenario: 同 callee 同参数重复调用消重
- **WHEN** `int F(int a){ return sq(a) + sq(a); }`（sq 纯），用 `Opt.PureCall`
- **THEN** IR 只剩**一条** `call @sq`，第二次复用首个 Dst

#### Scenario: 关闭时保留两次调用
- **WHEN** 同源码用 `Opt.None`
- **THEN** IR 有**两条** `call @sq`

#### Scenario: 非纯函数不消重
- **WHEN** callee 非纯（如写字段），`Opt.PureCall`
- **THEN** IR 仍两条 call（保守）

#### Scenario: 参数不稳定不消重
- **WHEN** 两次调用之间参数寄存器被重赋值（非单赋值/形参被改）
- **THEN** 不消重（key 要求全 args 稳定）

### Requirement: 纯调用 LICM 外提

#### Scenario: 循环不变纯调用提到 pre-header
- **WHEN** `int G(int n,int k){ int s=0; int i=0; while(i<n){ s=s+sq(k); i=i+1; } return s; }`，`Opt.PureCall`
- **THEN** `call @sq` 出现在循环 pre-header（每次进循环算一次）；`Opt.None` 时在循环体内

#### Scenario: 参数循环变则不外提
- **WHEN** 循环内 `sq(i)`（i 是归纳变量、循环内变）
- **THEN** 不外提（args 非循环不变）

### Requirement: 无格式变更 + 语义不变

#### Scenario: zbc 无格式 bump
- **WHEN** 编译任意含纯调用的代码
- **THEN** zbc/zpkg version 不 bump（纯编译期 IR 变换，`PureTable` 是内存分析、不序列化）

#### Scenario: 开/关输出一致（正确性主门）
- **WHEN** 运行时 golden 用 `--opt -pure-call` vs 默认
- **THEN** 程序输出**逐字节相同**

## MODIFIED Requirements

### Requirement: IsPure 白名单（不变）
**Before:** `IsPure` 排除所有 `Call`（当有副作用保留）。
**After:** `IsPure` **仍不放行** `Call`（它无 PureTable 上下文、无法判）；纯调用优化走**独立分支**
（`CseKey` 的 CallInstr 分支 + `IrLicm._isHoistablePureCall`），由 `IrPureFunctionTable` 提供纯度、
`Opt.PureCall` 门控。

## IR Mapping
- 纯 `CallInstr` → 优化后可能被 CSE remap 消除、或 LICM 移到 pre-header 块；序列化到 zbc 的仍是普通
  `call`（无新 opcode/字段）。`IrPureFunctionTable` 是编译期内存分析，不入 zbc。

## Pipeline Steps
- [ ] IR 分析（新 `IrPureFunctionTable.Compute` 模块不动点）
- [ ] IR Opt（CSE + LICM pure-call 分支，`Opt.PureCall` 门控）
- [ ] VM interp（无需改——优化在编译期，VM 执行普通 call）
