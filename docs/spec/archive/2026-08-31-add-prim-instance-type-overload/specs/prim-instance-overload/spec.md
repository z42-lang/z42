# Spec: prim 类型实例方法 type-based 重载

> 本 change 只覆盖阶段 1（编译器 support）。String 方法本身（阶段 2）不在本 spec。

## ADDED Requirements

### Requirement: prim 接收者实例方法支持 type-based 重载绑定

对 prim 包装类（`Std.String` / `Std.Int32` / `Std.Char` 等，static-imported 自 z42.core）的实例方法调用，当同名方法存在**同 arity、不同参数类型**的多个重载时，z42c 必须按**实参类型**决议出唯一目标，并绑定到该目标符号的 mangle `RegKey`（与 class 接收者路径一致），而非落到裸名 + Unknown 的 loose 绑定。

#### Scenario: prim 类同 arity type-based 实例重载绑定到正确 mangle 键
- **WHEN** 某 prim 包装类（如 `Std.String`）声明 `Split(string)` 与 `Split(char[])`（均 arity 1，MemberCollector 因 `arityDup==2` 将二者 mangle 成不同 RegKey），源码里对一个 `string` 值调用 `s.Split(someCharArray)`
- **THEN** 绑定结果为 `BoundCall("instance", ..., OwnerClass=PrimModel.Keyword("string"), MethodName=<`Split(char[])` 的 RegKey>, ..., ret=<该重载真实返回类型>)`，**不**落 loose-bind（`MethodName` 非裸 "Split"、`ret` 非 `Z42UnknownType`）

#### Scenario: 决议无歧义命中真实返回类型
- **WHEN** 上述调用按实参类型唯一匹配到一个重载
- **THEN** `BoundCall` 的类型 = 该重载 `Signature.Ret`（如 `Split(char[])` → `string[]`），下游 codegen 的 dst tag 为真实类型（非 Unknown）

#### Scenario: 无 type-based 重载时行为字节不变
- **WHEN** prim 类某方法**没有**同 arity type-based 重载（唯一方法、或仅 arity 不同的重载，如今天的 `Split(string)` / `Split(string,int)`）
- **THEN** 绑定走既有 `_overloadKey`+`_findMethod` 路径（`wms!=null`），产出的 `BoundCall`（OwnerClass / MethodName / 类型）与本 change 前**逐字节相同**——追加的类型决议分支不触发

#### Scenario: 真正未定义的 prim 方法仍 loose-bind Unknown
- **WHEN** 对 prim 接收者调用一个 prim 包装类里**根本不存在**的方法名（`_resolveOverload` 收集到 0 候选）
- **THEN** 仍落既有 loose-bind：`BoundCall(..., MethodName=裸名, ret=Z42UnknownType())`（追加分支不改变此兜底，运行期经 DepIndex 解析——既有行为）

### Requirement: 本地 prim 类不被实例 DepIndex 捷径劫持（local-wins 对称守卫）

编译某包时，对**本包自有** prim 包装类（在 `LocalClasses`，如编译 z42.core 时的 `String`）的实例方法调用，不得被 DepIndex 里下游同名实例方法劫持；本地 prim 类恒走本地 emit（VCall），与静态路径的 local-wins 守卫对称。

#### Scenario: 编译 z42.core 时本地 String 调用不误绑下游 Regex.Split
- **WHEN** 编译 z42.core，String.z42 内对 `string` 值调用一个 String 自有实例方法，且下游某包（如 `Std.Regex`）有同名同 arity 实例方法（`Regex.Split(string)`）
- **THEN** 该调用绑到本地 String 的方法（VCall / 正确 RegKey），**不**进 DepIndex 捷径、**不**登记 `Std.Regex` 为依赖 ns → 不触发 E0436（`namespace Std.Regex is used but not imported`）

#### Scenario: 非本地 prim 接收者仍可走 DepIndex 捷径
- **WHEN** 对一个非本包声明的 prim 相关调用（`ownerIsLocalInst==false`）且链上无自有方法
- **THEN** 保持既有 DepIndex 实例捷径行为（`GetInstance` → FQ Call），守卫不影响此路径

## Pipeline Steps

受影响阶段（本 change 为语义绑定 + codegen 层，不触及 lex/parse，不改类型系统规则）：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker（类型系统规则）— 无（不新增类型/规则）
- [x] 符号收集（`MemberCollector`）— 只读依赖：同 arity 重载 mangle 键已由既有逻辑产出（`:203-211`），本 change 不改，但绑定必须消费它
- [x] 成员绑定（`MemberResolver` prim-wrapper 分支）— 主修 B：`wms==null` 时追加 `_resolveOverload` 类型决议 → 正确 RegKey + 真实返回类型
- [x] codegen 调用发射（`CallEmitter` 实例路径）— 辅修 A：DepIndex 捷径加 local-wins 守卫；主修 B 修好后本地 prim 调用携正确 RegKey 走 VCall
- [ ] 运行期派发（VM VCall vtable）— **只读 / 待实测**：需坐实 VM 以 RegKey 为 prim 接收者 vtable 派发键（见 design Testing / proposal Open Questions Risk#3）

## IR Mapping

无。不新增 IR 指令、不改 zbc / zpkg 格式。type-based 重载沿用既有 RegKey（mangle 键）派发机制——class 接收者今天已用（`BoundCall.MethodName = ms.RegKey`），本 change 让 prim 接收者产出同款 RegKey 携 VCall。产物中方法仍以既有 mangle 键 / FQ 名表达。
