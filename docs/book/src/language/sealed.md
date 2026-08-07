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

## 去虚化（devirtualization，add-sealed-devirt）

sealed 类不可被继承 → 静态类型是 sealed 类 `A` 的 receiver，运行期实际类型**必然是 `A`** → `a.M()` 的
目标**编译期唯一可知**。编译器据此把 `VCallInstr` 降级为**直接 `CallInstr`**，解锁 `IrInline` 内联
（virtual 方法的 `VCall` 内联 pass 吃不进）。

- **净增价值 = 解锁内联**，不是派发提速——解释器已有多态内联缓存（`VCallIC`），单态 sealed 调用点派发已近直接调用。
- **落点**：`ExprEmitter._emitCall`（instance 分支）emit 时就地——天然在 `IrInline` 之前。`Opt.Devirt` 门控
  （release 全开；`--no-opt devirt` 关，供 before/after 逐字节对拍）。
- **目标解析**：`EmitContext.ResolveSealedTarget` 沿 sealed 类基链找**最近声明该方法且非 abstract 的本地非泛型类**
  `C`，产出 `QualifyClass(C) + "." + RegKey`——逐字节匹配 IrGen 的函数命名。`BoundCall.MethodName` 已是
  MemberResolver 解析后的 `ms.RegKey`（重载已消歧），故无需重解析。
- **v1 边界**：仅 **本地非泛型 sealed 类** receiver + 本地定义类 + 非 abstract 目标。**任何解析不确定即回落
  `VCallInstr`**（`ResolveSealedTarget` 返回 ""）——永不 miscall。
- **正确性铁律**：目标名错 = 静默调错。双保险：① 越界回落 VCall；② `--no-opt devirt` before/after 逐字节对拍
  + z42c 自举不动点（z42c 自身大量 sealed 类被去虚化编译，gen1==gen2 覆盖全码库）。

## Deferred / Future Work

### sealed-devirt-future: 去虚化 v1 边界外

- **来源**：`add-sealed-devirt` v1。
- **触发原因**：目标名精确构造在这些情形更易错，v1 保守只覆盖本地非泛型。
- **待覆盖**：① **imported sealed 类**（跨包目标名 + imported RegKey 约定，DepIndex 路径）；
  ② **泛型 sealed 类**（`$N` mangle + 类型参数替换）；③ **`sealed override` 方法**（receiver 是基类型——
  非「receiver 静态即 sealed 类」充分条件，需精确类型/单实现分析）。
- **当前 workaround**：这些情形回落 VCall（运行期仍正确，走 PIC），只是不内联。
