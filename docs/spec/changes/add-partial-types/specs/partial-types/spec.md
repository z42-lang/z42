# Spec: partial 类型

## ADDED Requirements

### Requirement: partial 类型声明与合并

同一 namespace、同一包内，一个类型可由多个 `partial` 声明拼成；编译期合并为单一类型。

#### Scenario: 两碎片合并字段与方法
- **WHEN** `a.z42` 有 `partial class Foo { public int X; public int GetX() { return X; } }`，
  `b.z42` 有 `partial class Foo { public int Y; public int GetY() { return Y; } }`（同 ns 同包）
- **THEN** 合并后 `Foo` 同时拥有字段 `X`/`Y` 与方法 `GetX`/`GetY`；`GetX` 可引用 `X`，`GetY` 可引用 `Y`
- **AND** zbc 中 `Foo` 是**单条完整 TYPE record**（含 `X`/`Y` 有序），格式与非 partial 类型无差异

#### Scenario: 缺 partial 修饰符报错
- **WHEN** `a.z42` 有 `partial class Foo {...}`，`b.z42` 有 `class Foo {...}`（未写 `partial`）
- **THEN** 编译错误：同名类型的所有声明必须均标 `partial`

#### Scenario: 合并顺序确定性
- **WHEN** 同一组 partial 碎片文件以任意文件系统枚举序被发现
- **THEN** 合并后的字段/方法顺序恒定（按项目相对路径 Ordinal 序 + 文件内声明序），产物逐字节相同

#### Scenario: 四种类型种类均支持
- **WHEN** partial 修饰的类型为 class / struct / record / interface 之一
- **THEN** 均按同一合并机制处理；所有碎片的 `Kind` 必须一致，否则报错

#### Scenario: 跨 namespace / 跨包同名不合并
- **WHEN** 两个同名 `partial class Foo` 分属不同 namespace 或不同包
- **THEN** 不合并（维持现有跨包 local-wins 语义）；跨包扩展类型走 `impl Trait for Type`

### Requirement: partial 类型的基类、构造器、接口

#### Scenario: 基类/主构造器至多一碎片声明
- **WHEN** 两个碎片都声明基类（`: Base`）或都带主构造器
- **THEN** 编译错误：基类/主构造器至多由一个碎片声明

#### Scenario: 接口列表取并集
- **WHEN** 碎片 A 声明 `: IA`，碎片 B 声明 `: IB`
- **THEN** 合并后 `Foo` 实现 `IA` 与 `IB`（按名 dedup）

#### Scenario: 重复成员报错
- **WHEN** 两碎片声明同名字段，或同签名的普通方法
- **THEN** 编译错误：成员重复定义

### Requirement: partial method（C# 9+ 干净形态）

声明与实现可分处两个碎片；允许任意返回类型、访问修饰符、`out`/`ref` 参数；无实现时整体擦除。

#### Scenario: 声明 + 实现配对
- **WHEN** 碎片 A `partial int Compute(int n);`，碎片 B `partial int Compute(int n) { return n*2; }`
- **THEN** `Compute` 作为正常方法编译；调用返回 `n*2`

#### Scenario: 无实现时擦除
- **WHEN** 只有声明 `partial void OnInit();`，无任何碎片提供实现
- **THEN** 不发方法桩；对 `OnInit()` 的无返回值/无 out 调用被静默消解（方法视为不存在）

#### Scenario: 声明与实现签名不一致报错
- **WHEN** 声明 `partial int M(int a);`，实现 `partial int M(string a) { ... }`
- **THEN** 编译错误：partial method 声明与实现签名必须完全一致

### Requirement: 与文件级增量编译共存

#### Scenario: 改一碎片联动重编同类型碎片
- **WHEN** partial 类型 `Foo` 由 `a.z42`/`b.z42` 组成，仅 `a.z42` 被修改
- **THEN** `a.z42` 与 `b.z42` 一起重编，重新发出合并后的完整 `Foo` TYPE record
- **AND** 增量构建的 dist 与全量构建的 dist **逐字节相同**

#### Scenario: 非 partial 文件不受牵连
- **WHEN** 修改某 partial 碎片
- **THEN** 与该 partial 类型无依赖关系的其它源文件的 zbc **不被重写**（mtime 不变）

## Pipeline Steps

受影响的 pipeline 阶段（按顺序）：
- [x] Lexer（新关键字 `partial`）
- [x] Parser / AST（类型 + 方法 partial 修饰符；`ClassDecl.IsPartial` / `MethodDecl` partial+body 位）
- [x] TypeChecker / SymbolCollector（碎片合并、冲突检测、partial method 配对）
- [x] IR Codegen（合并 TYPE record，主碎片发出；partial method 擦除）
- [ ] VM interp（**无改动**——TYPE record 与非 partial 同构）

## IR Mapping

- **无新 IR 指令、无新 zbc/zpkg opcode、无格式版本 bump**。
- partial 类型的 zbc `TYPE` record 与非 partial 类型**结构完全相同**（合并在编译期完成）。
- partial method：有实现 → 与普通方法相同的 `SIGS` + `FUNC` 发射；无实现 → 不发射。
