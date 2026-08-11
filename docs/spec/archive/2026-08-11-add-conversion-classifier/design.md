# Design: 统一类型转换分类器

## Architecture

```
                     ┌──────────────────────────────┐
调用点（PR1 仅一处）  │  Conversion.Classify(from,to, │
  _isAssignable ────▶│      symbols) → ConvResult    │
                     │  { int Kind; MethodSymbol? }  │
                     └───────────────┬───────────────┘
                                     │ 复用现有构件
             ┌───────────────────────┼───────────────────────────┐
             ▼                       ▼                           ▼
   Z42Type.IsAssignableTo    symbols.IsSubclassOf/Implements   数值矩阵
   （identity/widening/       （继承 / 接口）                 （拓宽 vs 窄化
    结构相等）                                                  vs 有损浮点）
```

PR1 只把 `_isAssignable` 一处接上分类器；cast 绑定 / `BoxIfNeeded` / codegen **不接**
（它们在 PR2/PR3 有行为变化时才接）。分类器**内部复用**现有 `IsAssignableTo` 等构件——
它是集中 + 打标签，不是重写判定逻辑。

## Decisions

### Decision 1: 分类器放语义层新文件，而非塞进 TypeFactsTc / Z42Type
**问题：** 转换分类逻辑放哪。
**选项：** A — 塞进 `TypeFactsTc`（已有 `_isAssignable`）；B — 塞进 `Z42Type.IsAssignableTo`
虚方法；C — 独立 `Conversion.z42`。
**决定：** 选 C。分类器要携带 `ConvResult`（种类 + 方法）、要被 PR2/PR3 多处消费（赋值 / cast /
box / 用户转换查找），是一个独立关注点；塞进 `TypeFactsTc`（本就杂）或 `Z42Type`（无 symbols
上下文，且是纯结构类型对象）都违反单一职责。独立文件也便于 PR2/PR3 增量扩展。

### Decision 2: PR1 保持"宽松门"，把种类标"正确"但不据此收紧
**问题：** 窄化 / 有损浮点在 PR1 该标成什么、门该不该收。
**选项：** A — PR1 直接标 ImplicitNumeric（把窄化当隐式，等 PR2 再改标签）；B — PR1 就标
ExplicitNumeric（正确），但门（`ImplicitOkPermissive`）临时把 ExplicitNumeric 也放行。
**决定：** 选 B。种类从一开始就**语义正确**（窄化本就是显式转换），PR2 无需改分类逻辑、只
改门的白名单（去掉 ExplicitNumeric）。这把"分类"与"执行策略"解耦：分类是事实，门是策略。
符合根因修复——PR2 不是"重新分类"，而是"策略切换"。

### Decision 3: `ConvKind` 用 `static class + int` 常量（不用 enum）
**问题：** 种类枚举怎么表达。
**决定：** 沿用 z42c 既有写法（`TokenKind` / `DiagnosticCodes` 同款 `public static int X = n;`，
z42c 受限写法不用 enum）。`ConvResult` 是 `sealed class`（带 `Kind` + `Method` 字段 + 投影助手）。

### Decision 4: 有损浮点判定按"尾数位宽"
**问题：** 哪些整数→浮点算有损（标 ExplicitNumeric）。
**决定：** 采用 C# 隐式数值矩阵，**剔除**尾数放不下的整数→浮点项：`int/uint→float`、
`long/ulong→float`、`long/ulong→double` 标 ExplicitNumeric；其余（`byte/short/char→float`、
`int/uint→double`、`float→double`）仍 ImplicitNumeric。判定用一张小查找表（from,to→kind），
镜像并取代 `Z42PrimType._canWiden`（`_canWiden` 保留供 `IsAssignableTo` 结构判定，分类器
新表更细，区分无损/有损；两者在 PR1 的布尔投影上一致——见等价性验证）。

> 注意：`_canWiden` 现把 `int→float`、`long→float/double` 判为拓宽=true。分类器把它们标
> ExplicitNumeric，但 `ImplicitOkPermissive()` **仍放行** ExplicitNumeric → 布尔投影一致，
> byte-identical 不破。PR2 收紧门后，这些点才需要显式 cast（并迁移调用方）。

## Implementation Notes

- `Conversion.Classify` 判定顺序（短路，镜像旧 `_isAssignable` 分支序以保证等价）：
  1. 任一侧 error/unknown → `Absorb`
  2. 恰一侧泛型形参 → `GenericErase`
  3. `to` 是 `object` → 值 prim 源 `Boxing`，否则 `ImplicitRef`（对应旧"任意→object 放行"）
  4. 两侧数值 prim → 查数值矩阵（Identity / ImplicitNumeric / ExplicitNumeric）
  5. `from.IsAssignableTo(to)` → Identity（同名 / 结构 / 数组 / func / 别名）
  6. class→class subclass / class→iface / instantiated→iface·base / class→instantiated →
     `ImplicitRef`
  7. 值 prim → 接口 → `Boxing`；`object`/接口 → 值 prim → `Unboxing`
  8. 否则 `None`
- `ImplicitOkPermissive()` = `Kind ∈ {Absorb, GenericErase, Identity, ImplicitNumeric,
  ExplicitNumeric, Boxing, ImplicitRef}`。
- **等价性是硬约束**：改完先跑 `xtask test compiler`（自举）+ 全 golden；任何 golden 输出变化
  或 gen1≠gen2 = 分支序 / 判定与旧 `_isAssignable` 有偏差，必须修回等价。

## Testing Strategy

- **单元测试** `tests/conversion/conversion_tests.z42`：逐 `ConvKind` 断言 `Classify` 的种类
  标签（prim 组合用空 `SymbolTable`；继承/接口用构造好的 symbols）；断言 `ImplicitOkPermissive()`
  与预期布尔一致。覆盖 spec 全部 Scenario。
- **等价 / 不动点**：`xtask test`（完整 GREEN gate）——重点看 `xtask test compiler`（自举 5/5 +
  gen1==gen2 字节不动点）与 e2e / stdlib / cross-zpkg 全 golden 输出**零变化**。这是 byte-identical
  纯重构的黄金验证。
