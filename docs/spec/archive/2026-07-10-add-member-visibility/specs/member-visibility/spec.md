# Spec: 成员可见性元数据

## ADDED Requirements

### Requirement: 字段/方法可见性进 TYPE/SIGS + 反射

#### Scenario: 字段可见性 emit
- **WHEN** z42c 编译 `class C { public int a; private int b; int c; }`
- **THEN** TYPE 字段块每字段带 visibility u8:a=0(public)、b=1(private)、c=0(默认 public)

#### Scenario: 方法可见性 emit
- **WHEN** `public void M(){} private void N(){}`
- **THEN** SIGS 对应函数在 is_static 后带 visibility:M=0、N=1

#### Scenario: FieldInfo.IsPublic 反射
- **WHEN** `typeof(C).GetFields()` 取到字段 a / b 的 FieldInfo
- **THEN** `a.IsPublic==true && a.IsPrivate==false`;`b.IsPublic==false && b.IsPrivate==true`

#### Scenario: MethodInfo.IsPublic 反射
- **WHEN** `typeof(C).GetMethods()` 取到 M / N
- **THEN** `M.IsPublic==true`;`N.IsPublic==false && N.IsPrivate==true`

#### Scenario: 默认 public
- **WHEN** 字段/方法无显式修饰符
- **THEN** visibility=0(public)——沿用 z42 现有默认（不改语义）

#### Scenario: 静态字段可见性
- **WHEN** `public static int s; private static int t;`
- **THEN** 静态字段块每字段亦带 visibility；反射 IsPublic 正确

#### Scenario: strict-pin
- **WHEN** 旧 minor reader 遇新产物 → 拒绝(regen 后新 reader 正常)

#### Scenario: 派发/现有反射不回归
- **WHEN** 现有 vcall / GetFields / GetMethods
- **THEN** 行为不变(visibility 只是新增字段,不改派发/成员枚举逻辑)

## Pipeline Steps
- [x] SymbolCollector/AST（可见性来源 Mods，已有）
- [x] IR Codegen（IrFieldDesc/IrFunction.Visibility；ZbcWriter TYPE/SIGS）
- [x] VM interp（read_type/read_sigs visibility + 反射 builtin）
