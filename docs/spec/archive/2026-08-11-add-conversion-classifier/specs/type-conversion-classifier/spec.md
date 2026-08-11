# Spec: 类型转换分类器（Conversion classifier）

## ADDED Requirements

### Requirement: 转换分类 API

提供 `Conversion.Classify(from, to, symbols)`，把一对类型 `(from → to)` 分类为一个
`ConvResult{Kind, Method}`，其中 `Kind` 取自 `ConvKind` 常量集，`Method` 仅在用户转换
（PR3）时非 null，PR1 恒为 null。

`ConvKind` 常量集（PR1 全部落地，User* 两项为 PR3 预留但已定义）：

| 常量 | 含义 | PR1 是否隐式可赋 |
|------|------|:---:|
| `None` | 不存在任何转换 | ✗ |
| `Absorb` | 任一侧 error/unknown（吸收，防级联） | ✓ |
| `GenericErase` | 恰一侧是泛型形参（类型擦除放行） | ✓ |
| `Identity` | 规范化同型（剥 `?` + 别名后名等价） | ✓ |
| `ImplicitNumeric` | 无损数值拓宽 | ✓ |
| `ExplicitNumeric` | 数值窄化 **或** 有损浮点 | ✓（PR1 宽松；PR2 起 ✗） |
| `Boxing` | 值类型 → `object`/接口 | ✓ |
| `Unboxing` | `object`/接口 → 值类型 | ✗ |
| `ImplicitRef` | 引用上转（派生→基、类→接口、`null`→引用、任意→`object`） | ✓ |
| `ExplicitRef` | 引用下转（基→派生） | ✗ |
| `UserImplicit` | 用户 `implicit operator`（PR3） | ✓ |
| `UserExplicit` | 用户 `explicit operator`（PR3） | ✗ |

#### Scenario: 无损数值拓宽 → ImplicitNumeric
- **WHEN** `Classify(int, long)` / `Classify(int, double)` / `Classify(byte, int)` / `Classify(float, double)`
- **THEN** `Kind == ImplicitNumeric`

#### Scenario: 数值窄化 → ExplicitNumeric
- **WHEN** `Classify(long, int)` / `Classify(int, byte)` / `Classify(double, int)`
- **THEN** `Kind == ExplicitNumeric`

#### Scenario: 有损浮点 → ExplicitNumeric（比 C# 严；PR2 收紧的目标）
- **WHEN** `Classify(int, float)` / `Classify(long, float)` / `Classify(long, double)` / `Classify(ulong, double)`
- **THEN** `Kind == ExplicitNumeric`

#### Scenario: 同型 → Identity
- **WHEN** `Classify(int, int)` / `Classify(byte, u8)`（别名）/ `Classify(int?, int)`（剥 nullable）
- **THEN** `Kind == Identity`

#### Scenario: 装箱 / 拆箱方向
- **WHEN** `Classify(int, object)` → **THEN** `Kind == Boxing`
- **WHEN** `Classify(object, int)` → **THEN** `Kind == Unboxing`

#### Scenario: 错误吸收
- **WHEN** from 或 to 是 `Z42ErrorType` / `Z42UnknownType`
- **THEN** `Kind == Absorb`

#### Scenario: 不相关类型 → None
- **WHEN** `Classify(int, string)` / `Classify(bool, int)`
- **THEN** `Kind == None`

### Requirement: 布尔投影行为等价（byte-identical 保证）

`ConvResult.ImplicitOkPermissive()` 返回该转换在 **PR1 宽松门**下是否隐式可赋——
即 `Kind ∈ {Absorb, GenericErase, Identity, ImplicitNumeric, ExplicitNumeric, Boxing, ImplicitRef}`。
`TypeFactsTc._isAssignable` 改为返回此投影。

#### Scenario: 与旧 _isAssignable 逐位等价
- **WHEN** 对任意 `(from, to, symbols)` 组合
- **THEN** `Conversion.Classify(from, to, symbols).ImplicitOkPermissive()` 的返回值与本变更前
  `_isAssignable(from, to, symbols)` **完全相同**（含 error/unknown 吸收、泛型擦除、任意→object、
  继承/接口、数值双向放行等所有既有分支）

#### Scenario: 自举字节不动点
- **WHEN** 用本变更后的 z42c 自编译 z42c 源码两代（gen1、gen2）
- **THEN** gen1 与 gen2 产物**逐字节相同**，且全部 golden / stdlib / cross-zpkg 测试输出不变

## MODIFIED Requirements

### Requirement: 可赋性判定的实现路径

**Before:** `TypeFactsTc._isAssignable(from, to, symbols)` 内联一串 `if`（error 吸收、泛型擦除、
任意→object、`IsAssignableTo`、subclass/implements、数值双向放行）直接返回 bool。

**After:** `_isAssignable` 委托给 `Conversion.Classify(from, to, symbols).ImplicitOkPermissive()`；
上述判定逻辑集中进 `Conversion.Classify`，并额外产出种类标签。**外部可观察行为不变**。

## IR Mapping

无。PR1 不新增 / 不改任何 IR 指令，不改 zbc/zpkg 格式，不插入任何转换指令。

## Pipeline Steps

受影响阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [x] TypeChecker — `_isAssignable` 改走分类器（行为不变）；新增 `Conversion` 分类器
- [ ] IR Codegen — 无
- [ ] VM interp — 无
