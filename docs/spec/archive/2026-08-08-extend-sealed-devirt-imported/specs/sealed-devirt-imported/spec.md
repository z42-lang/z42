# Spec: 去虚化扩到 imported sealed 类

## ADDED Requirements

### Requirement: imported sealed 类 receiver 去虚化

#### Scenario: imported sealed 类自身声明的 virtual 方法
- **WHEN** pkgB 中 `A a = ...; a.M();`，`A` 是 pkgA 导出的**非泛型** `sealed class`，`M` 是 `A` 上有 body 的 virtual/override
- **THEN** 编译器发射直接 `CallInstr(dst, "<pkgA.ns>.A.M", [a, ...args], argc+1)`（目标 = 导出包发射的函数名）而非 `VCallInstr`；可被 `IrInline` 内联

#### Scenario: imported sealed 类继承 imported 基类方法
- **WHEN** pkgA 有 `class B { virtual M }` 与 `sealed class A : B {}`（A 不 override M），pkgB `A a; a.M();`
- **THEN** 目标解析到 `B.M`：`CallInstr(dst, "<pkgA.ns>.B.M", ...)`

#### Scenario: imported sealed 类继承**另一包**基类方法（跨包基链）
- **WHEN** pkgBase 出 `class Tagged { virtual Tag }`，pkgSealed 出 `sealed class Leaf : Tagged {}`（不 override
  Tag），pkgApp `Leaf lf; lf.Tag();`。注：imported `Leaf.Methods` 因 TSIG 展平**含**继承来的 `Tag`。
- **THEN** 目标**不得**解析到 `pkgSealed.ns.Leaf.Tag`（从未发射）。必须沿基链上溯到真正声明类：
  `CallInstr(dst, "<pkgBase.ns>.Tagged.Tag", ...)`。判据 = 构造的 FQ 须命中 `Deps.Statics`（真实发射函数）；
  未命中即继续上溯。

#### Scenario: imported sealed override 自身声明（不上溯）
- **WHEN** pkgSealed 出 `sealed class Circle : Shape { override Area }`，pkgApp `Circle c; c.Area();`
- **THEN** `<pkgSealed.ns>.Circle.Area` 命中 `Deps.Statics`（Circle 真发射了 Area override）→ 目标即它，
  `CallInstr(dst, "<pkgSealed.ns>.Circle.Area", ...)`

#### Scenario: 去虚化前后输出逐字节相同
- **WHEN** 同一含 imported sealed 调用的程序以 `Opt.Devirt` 开 / 关编译运行
- **THEN** stdout 逐字节相同

### Requirement: 越 v1 边界的 imported receiver 回落 VCall

#### Scenario: imported 泛型 sealed 类 → 仍 VCall
- **WHEN** receiver 静态类型是 imported **泛型** sealed 类
- **THEN** `SealedReceiverClass` 返回 null（非泛型铁律）→ `VCallInstr`

#### Scenario: 定义类既非本地也不在 ImportedClassNs → 回落
- **WHEN** 沿基链解析到的定义类无法经 `_devirtQualifiable` 确定 ns（QualifyClass 无法给出正确 FQ）
- **THEN** `ResolveSealedTarget` 返回 "" → `VCallInstr`（保守，永不 miscall）

### Requirement: 本地路径不回归

#### Scenario: 本地 sealed devirt 行为不变
- **WHEN** receiver 是本地非泛型 sealed 类（#142 覆盖的情形）
- **THEN** 去虚化行为与 #142 完全一致（`QualifyClass`=当前 ns，目标名不变）

### Requirement: imported 定义类候选须经 Deps 校验 FQ 真实发射

#### Scenario: 构造的 imported FQ 不在 Deps → 继续上溯
- **WHEN** 沿基链某 imported 类 `ct` 的 `Methods` 含 methodKey，但 `QualifyClass(ct)+"."+methodKey` **不在**
  `Deps.Statics`（即 `ct` 只经 TSIG 展平继承了该方法、未真声明）
- **THEN** 不在 `ct` 返回，继续 `while` 上溯基链，直到某祖先的 FQ 命中 `Deps.Statics`（或越界回落 VCall）

#### Scenario: 本地类命中直接返回（不经 Deps 校验）
- **WHEN** 定义类是本地类（`LocalClasses.ContainsKey`）且 `Methods` 含 methodKey
- **THEN** 直接返回 `QualifyClass(本地类)+"."+methodKey`（本地声明即发射；本包函数不入 Deps，不能也不需 Deps 校验）

## IR Mapping

- 同 #142：`VCallInstr` → `CallInstr(dst, QualifyClass(定义类)+"."+RegKey, [recv,...args], argc+1)`；
  imported 时 `QualifyClass` 经 `ImportedClassNs` 给源 ns。**imported 定义类**额外经 `Deps.Statics.ContainsKey(FQ)`
  校验真实发射（排除 TSIG 展平的继承方法，见 design Decision 2.5）。无新 IR 指令、无格式变化。

## Pipeline Steps

- [x] Lexer / Parser / TypeChecker —— 无改动
- [x] IR Codegen —— `EmitContext.SealedReceiverClass`/`ResolveSealedTarget` 守卫放宽 + `_devirtQualifiable`
  + `_depHasFunction`（imported 定义类经 `Deps.Statics` 校验 FQ 真实发射，排除 TSIG 展平的继承方法）
- [x] VM interp —— 无改动
