# Design: tighten-implicit-conversions（PR2）

## D1 执行门收紧

`Conversion.z42`：`ImplicitOkPermissive()` 更名 `ImplicitOk()`，白名单从

```
{Absorb, GenericErase, Identity, ImplicitNumeric, ExplicitNumeric, Boxing, ImplicitRef, UserImplicit}
```

剔除 `ExplicitNumeric` →

```
{Absorb, GenericErase, Identity, ImplicitNumeric, Boxing, ImplicitRef, UserImplicit}
```

`TypeFactsTc._isAssignable(from, to, symbols)` 保持**纯类型**判定（不看表达式），改调 `ImplicitOk()`。
窄化 / 有损浮点 → 返回 `false`。**注意**：`_isAssignable` 被重载候选决议等多处复用，纯类型收紧后
窄化实参不再"可赋" = 不参与该候选（与 C# 一致）。常量例外**不**进 `_isAssignable`（它没有表达式），
只在下面 `CheckImplicitConvert` 的调用点生效。

## D2 常量在范围内例外（C# 规则，仅整数目标）

隐式赋值/return/传参的**检查**从「裸 `_isAssignable` + 报 TypeMismatch」升级为一个**表达式感知**的
集中助手（`TypeChecker.CheckImplicitConvert`）：

```
// 返回 true=可隐式（放行），false=已报诊断
bool CheckImplicitConvert(BoundExpr value, Z42Type target, SymbolTable syms, Span sp, string ctx) {
    ConvResult r = Conversion.Classify(value.Type(), target, syms);
    if (r.ImplicitOk()) { return true; }                       // Identity/ImplicitNumeric/Boxing/Ref...
    // 常量在范围内例外：整数目标 + 编译期常量整数源 + 值在目标范围
    if (r.Kind == ConvKind.ExplicitNumeric
        && _isIntegerPrim(target)
        && _constIntInRange(value, target)) { return true; }   // byte b = 48; ✓
    // 存在显式转换却用于隐式上下文 → E0439（缺 cast 提示）
    if (r.Exists()) {
        _diags.Error(E0439, "cannot implicitly convert '"+from+"' to '"+to+"'; "
                          + "an explicit conversion exists (are you missing a cast?)", sp);
        return false;
    }
    _diags.Error(E0402/TypeMismatch, "cannot assign "+from+" to "+to+" ("+ctx+")", sp);
    return false;
}
```

`_constIntInRange(value, target)`：
- `value` 归约为编译期整数常量值（PR2 覆盖：`BoundLitInt`、一元负号包裹的 `BoundLitInt`、
  已折叠成字面量的局部 `const`）。非常量 → 不适用。
- 目标整数 prim 的 `[min, max]`（i8/u8/i16/u16/i32/u32/i64/u64）判定值是否落域内。
- 落域 → true。越界（`byte b = 300`）→ false → 落到 E0439。

> **有损浮点无例外**：`_isIntegerPrim(target)` 为假 → `float f = 5;` 不走例外 → E0439（决策 3）。

## D3 ConvertInstr 插入（ConvertIfNeeded）

镜像 `BoxIfNeeded`，在同样的 4 处协变点调用（return/var-decl/assign/call-arg），**在 Box 之后**
（两者互斥：Box 针对 →object/接口，Convert 针对数值 prim→prim）：

```
BoundExpr ConvertIfNeeded(BoundExpr value, Z42Type target) {
    if (value == null || target == null) { return value; }
    Z42Type vt = value.Type();
    if (!(vt is Z42PrimType) || !(target is Z42PrimType)) { return value; }
    if (!_isNumericPrim(vt.Name()) || !_isNumericPrim(target.Name())) { return value; }
    if (_repClass(vt.Name()) == _repClass(target.Name())) { return value; }   // 同表示类 → no-op
    return new BoundConvert(value, target, value.Span);                        // → _emitConvert 发 ConvertInstr
}
```

`_repClass`（运行期表示类）：`f32/f64 → FLOAT`；`char → CHAR`；其余整数族 → `INT`。

| from→to | repClass | 插 ConvertInstr? | 理由 |
|---------|----------|:---:|------|
| `int→long`、`byte→int` | INT→INT | ✗ | 运行期同 `I64`，等宽拓宽 no-op |
| `int→double`、`uint→double` | INT→FLOAT | ✓ | `I64→F64` 真转（否则 I64 存进 F64 槽）|
| `f32→f64` | FLOAT→FLOAT | ✗ | 运行期同 `F64` |
| `char→int`、`char→double` | CHAR→INT/FLOAT | ✓ | `Value::Char` → I64/F64 真转 |

到达 `ConvertIfNeeded` 的值已过 `CheckImplicitConvert`（Identity 或 ImplicitNumeric 或常量例外的窄化），
故不会误插到被拒绝的转换上。**常量例外的窄化**（`byte b=48`）：repClass INT→INT → 不插（值 48 已在
范围，无需截断）——正确。

> `_emitConvert`（ExprEmitter.z42:1378）现有 `fromIr==toIr` 短路兜底再挡一层无操作，无需改动。

## D4 调用点改造

| 位置 | 现状 | 改为 |
|------|------|------|
| `StmtBinder._bindReturn` (152) | `if(!_isAssignable) Error(TypeMismatch)` + `BoxIfNeeded` | `CheckImplicitConvert(val, ret,…)` + `val=BoxIfNeeded(val,ret)` + `val=ConvertIfNeeded(val,ret)` |
| `StmtBinder._bindVarDecl` (307/185) | 同上 | 同上 |
| `ExprTyper._bindAssign` (164) | 同上 | 同上 |
| `OverloadBinder`（arg 协变 / 251 params 元素）| `_isAssignable` | resolved 后逐 arg `CheckImplicitConvert` + `ConvertIfNeeded`（复用 `BoxArgs` 路径加一趟）|

> 重载**候选选择**仍用纯类型 `_isAssignable`（窄化实参 = 该候选不适用，C# 同）。常量例外只在**已选定
> 候选**的最终 arg 检查/协变时应用——覆盖 `Foo((byte-param) 48)` 常量实参场景；若某罕见重载需常量例外
> 参与候选适用性，留 follow-up（IMPL 按实测定）。

## D5 迁移策略（grind）

先迁移、后收紧（同一 PR 内先改源再翻门，避免 z42c 自编不过）。grind 循环（参考
[[file-scoped-usings-migration]]）：

1. 翻门（收紧 `Conversion.z42`）。
2. `xtask build stdlib` → 抓 `E0439`/`E0402` 的 `file:line`。
3. 逐点判定：越界常量 / 非常量窄化 / 有损浮点隐式 → 包 `(T)`；在范围常量 → **不应报**（若报=常量例外
   实现漏了，回头修 `_constIntInRange`）。
4. 重编，迭代至 stdlib 绿；再对 z42c 源自身重复（自举 gen2 暴露）。

**预期迁移面**：小。实测 `Tar.z42` 的 12 处**全是**在范围常量（`0/48/53/32`）→ 常量例外后**归零**。
真需 cast 的是非常量窄化 / 越界 / 有损浮点隐式点，散见 binary/hash/encoding writer，量级预计十数处内。

## D6 验证

- 单测（`z42c.semantics/tests/conversion/`）：
  - 负向：`int→byte`（非常量）拒绝、`long→int` 拒绝、`float f = intVar` 拒绝。
  - 常量例外：`byte b = 48` 接受、`byte b = 300` 拒绝（E0439）、`sbyte x = -1` 接受。
  - 分类不变量：ImplicitNumeric 仍隐式（int→long/double）。
- e2e：截断 / 表示正确性（`(byte)300==44`、`double d=5` 真 F64 参与运算）。
- 自举：`xtask test` 收敛 gen_n==gen_{n+1}（破一代→warm 重建）。
- `xtask test bootstrap`：上一 nightly 编迁移后源无越界。

## D7 决策记录

- **常量在范围内隐式（C# 规则，仅整数目标）**：User 裁决 2026-08-11。理由：在范围常量窄化逐值可证
  无损，与「隐式只允许绝对无损」一致；且使 binary-writer 代码免于满屏 `(byte)`。有损浮点不含例外。
- **repClass 门控 ConvertInstr**：只在表示类变化时插，等宽整数拓宽 / f32→f64 不插——正确且最小字节扰动。
- **E0439 vs E0402**：存在显式转换 → E0439（提示补 cast）；根本无转换 → E0402/TypeMismatch。
