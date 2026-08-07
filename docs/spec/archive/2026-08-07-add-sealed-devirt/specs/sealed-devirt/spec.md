# Spec: 基于 sealed 的去虚化

## ADDED Requirements

### Requirement: sealed 类 receiver 的 virtual 调用去虚化

#### Scenario: sealed 类自身声明的 virtual 方法
- **WHEN** `A a = new A(); a.M();`，`A` 是本地非泛型 `sealed class`，`M` 是 `A` 上的 virtual/override 方法（有 body）
- **THEN** 编译器发射直接 `CallInstr(dst, "<Ns>.A.M", [a, ...args], argc+1)` 而非 `VCallInstr`；该 `Call` 可被 `IrInline` 内联

#### Scenario: sealed 类继承而未 override 的方法 → 目标是基类实现
- **WHEN** `sealed class A : B {}`，`B` 有 virtual `M`（A 不 override），`A a; a.M();`
- **THEN** 发射直接 `CallInstr(dst, "<Ns>.B.M", ...)`（目标解析到最近声明 M 的基类 B）

#### Scenario: 去虚化前后可观察输出逐字节相同
- **WHEN** 同一 sealed-receiver 程序分别以 `Opt.Devirt` 开 / 关（`--no-opt devirt`）编译并运行
- **THEN** 两次 stdout **逐字节相同**（去虚化是纯优化，不改语义）

### Requirement: 非 sealed / 越 v1 边界的 receiver 保持虚派发

#### Scenario: 非 sealed 类 receiver → 仍 VCall（override 生效）
- **WHEN** receiver 静态类型是非 sealed 类，运行期实际是其子类且 override 了方法
- **THEN** 发射 `VCallInstr`，运行期派发到子类 override（**证明未误去虚化**）

#### Scenario: 接口 / cast-to-class Unknown 链 receiver → 仍 VCall
- **WHEN** receiver 是接口类型，或 `((A)x).M()` 的 cast-to-class Unknown 链
- **THEN** 保持 `VCallInstr`（既有守卫优先于去虚化）

#### Scenario: v1 边界外（imported / 泛型 sealed）→ 回落 VCall
- **WHEN** receiver 静态类型是 imported sealed 类，或泛型 sealed 类，或目标定义类为 imported/泛型
- **THEN** `ResolveSealedTarget` 返回 ""，回落 `VCallInstr`（v1 不去虚化；运行期仍正确，走 PIC）

### Requirement: abstract 目标不去虚化

#### Scenario: 沿基链最近声明是 abstract
- **WHEN** sealed 类 receiver 的方法在基链最近声明处是 abstract（无 body）
- **THEN** 不去虚化（无直接调用目标）——回落 VCall（运行期派发到具体 override）

## IR Mapping

- 去虚化：`VCallInstr(dst, recv, method, args, argc)` → `CallInstr(dst, FQ(definingClass)+"."+RegKey, [recv, ...args], argc+1)`（`ExprEmitter._emitCall`，`Opt.Devirt` 门控）。
- 目标名 = `IrGen` 发射该函数的同一构造（`_q(_classIrShortName(C))+"."+md.RegKey`）。
- 无新 IR 指令、无 zbc/zpkg 格式变化（复用既有 `CallInstr`）。

## Pipeline Steps

- [x] Lexer / Parser —— 无改动
- [x] TypeChecker / SymbolCollector —— 无改动（消费既有 `IsSealed` 地基）
- [ ] IR Codegen —— `ExprEmitter._emitCall` devirt 分支 + `EmitContext.SealedReceiverClass`/`ResolveSealedTarget` + `Opt.Devirt`
- [ ] VM interp —— 无改动（直接 `Call` 既有语义；去虚化后结果不变）
