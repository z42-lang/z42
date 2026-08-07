# sealed 修饰符

> 对齐：2026-08-07（change `impl-sealed-semantics`）

`sealed` 封闭一个**类**（不可被继承）或一个**虚方法**（不可被再 override）。语义与 C# 一致——
`sealed` 是其它语言 `final`（Java / Kotlin / Swift / C++）的对应物；**z42 没有 `final` 关键字**。

## 语法

### sealed 类

```z42
public sealed class Cat : Animal {   // Cat 不可被继承
    public override string Name() => "Cat";
}

class Kitten : Cat {}   // ✗ E0427：cannot inherit from sealed class `Cat`
```

`sealed` 只禁止**类继承**，不禁止**实现接口**：

```z42
sealed class A : IFoo { public void F() {} }   // ✓ 合法
```

### sealed 方法

`sealed` 用在一个 override 上，封死后续 override：

```z42
class Base   { public virtual void M() {} }
class Mid    { public sealed override void M() {} }   // 到此为止
class Leaf : Mid { public override void M() {} }      // ✗ E0428：cannot override sealed method `M`
```

### 简写：方法上单写 `sealed`（z42 扩展）

z42 方法默认非虚，一个不 override 任何东西的方法本就不可被 override——对它 `sealed` 无意义。
因此**方法级 `sealed` 必然是 override**，`override` 可省：

```z42
class Dog : Animal {
    public sealed void Sound() => "Woof";   // 等价 `sealed override`，必须匹配基类 virtual
}
```

- 单写 `sealed` **等价** `sealed override`（绑定为对基类 virtual 的 override + 封死）。
- 仍接受显式 `sealed override`（C# 代码可原样粘贴）；`override` 视为允许的冗余。
- 这是 z42 相对 C# 的**单向超集**：C# 里裸 `sealed` 方法非法，z42 接受它作简写。

## 强制规则（语义检查）

| 情形 | 诊断 |
|------|------|
| 类继承一个 sealed 类 | **E0427** cannot inherit from sealed class |
| override 一个 sealed（override）方法 | **E0428** cannot override sealed method |
| 方法级 `sealed`（含简写）无匹配基类 virtual | **E0429** sealed method must override a base virtual method |

跨包同样强制：导入类型的 sealed 性经 TSIG（`ExportedClassZ.IsSealed` / `ExportedMethodZ.IsSealed`）
还原，源自 zpkg 的 `CLASS_FLAG_SEALED`（TYPE flags bit1）/ `METHOD_FLAG_SEALED`（SIGS method_flags bit2）。

## 反射

- `Type.IsSealed` —— 类是否 sealed（既有，`CLASS_FLAG_SEALED`）。
- `MethodInfo.IsSealed` —— 方法是否 sealed（新增，`METHOD_FLAG_SEALED`，zbc 1.30）。sealed 方法亦 virtual。

## 机制 / 实现

- **语义强制**：`SymbolCollector._passSealedEnforce`（在 `_passFixupOverrides` 后运行，签名/regKey 已定案）。
  - 继承检查：类的基 `Z42ClassType.IsSealed` → E0427。
  - override 检查：`_nearestBaseMethod` 沿基链找**最近**被覆盖方法，其 `IsSealed` → E0428。
  - 简写解析：方法 `Mods` 含 `sealed` 无 `override` 时，`_passFixupOverrides` 的 2 处 override 识别点
    亦认 `sealed` → 简写参与 vtable 槽对齐（否则 override 落新槽、虚派发打不到基实现）。
- **元数据**：`IrGenFacts._methodFlags` 从 `Mods` 置 `method_flags` bit2（并连带 bit0=virtual，因 sealed 必虚）。
  类级 sealed 沿用既有 `CLASS_FLAG_SEALED`（zbc 1.12，不新增）。
- **格式**：加方法级 sealed 位是 zbc 1.29→1.30 / zpkg 34→35 的语义扩展 bump（字节布局不变，bit2 先前保留为 0；
  strict-pin 下仍 bump 以防同版本号语义分歧，见 [version-bumping](../../../.claude/rules/version-bumping.md)）。

## Deferred / Future Work

### sealed-devirt: 基于 sealed 的去虚化

- **来源**：`impl-sealed-semantics` 拆分（原含此项）。
- **触发原因**：直接调用目标解析（定义类 + RegKey + abstract 跳过 + 本地/imported 约定）正确性敏感
  （错=静默 miscall），需独立测试矩阵，与语义强制耦合度低。
- **前置依赖**：已就绪——类级 `CLASS_FLAG_SEALED`、方法级 `METHOD_FLAG_SEALED`、本地+跨包
  `Z42ClassType.IsSealed`/`MethodSymbol.IsSealed`（本 change 全部落齐）。
- **触发条件**：follow-up change `add-sealed-devirt`。落点 `ExprEmitter._emitCall` + `EmitContext`
  新增目标解析：receiver 静态类型是 sealed 类时把 `VCallInstr` 降级为直接 `CallInstr`，解锁 `IrInline`。
- **当前 workaround**：无——运行时已有多态内联缓存（`VCallIC`），单态 sealed 调用点派发已近直接调用；
  去虚化的净增价值是**解锁内联**而非派发提速。
