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
- **目标解析**：`EmitContext.ResolveSealedTarget` 沿 sealed 类基链找**最近声明该方法且非 abstract 的可限定非泛型类**
  `C`，产出 `QualifyClass(C) + "." + RegKey`——逐字节匹配 IrGen 的函数命名。`BoundCall.MethodName` 已是
  MemberResolver 解析后的 `ms.RegKey`（重载已消歧），故无需重解析。「可限定」= `_devirtQualifiable(name)`：
  在 `LocalClasses`（本地，QualifyClass=当前 ns）**或** `ImportedClassNs`（imported，QualifyClass=源 ns）。

### imported sealed 去虚化的坑：TSIG 展平继承方法（extend-sealed-devirt-imported）

imported 类的符号 `Methods` 由 `ImportedSymbolLoader` 从 TSIG 重建，而 **TSIG 把继承方法展平进每个派生类**。
于是 `sealed class Leaf : Tagged {}`（Leaf 不 override `Tag`）在 imported 侧 `Leaf.Methods.ContainsKey("Tag")`
= **true**，naïve 地构造 `QualifyClass(Leaf)+".Tag"` = `Demo.Sld.Leaf.Tag`——**一个从未被任何包发射的函数名**
（真身是基类包的 `Demo.Base.Tagged.Tag`）→ 运行期 `undefined function`。本地类无此坑（`SymbolCollector` 只填
**声明于本类**的方法）。

**解法**：imported 定义类候选返回前，用 `Deps.Statics.ContainsKey(FQ)`（`_depHasFunction`）**校验该 FQ 确为
真实发射函数**——`DependencyIndex.AddModule` 把每个跨包函数按完整 FQ 注册进 `Statics`。命中 → 本类真声明 → 去
虚化；未命中 → 本类仅继承 → **沿基链继续上溯**到真声明类（其 FQ 命中）。本地类走 `LocalClasses` 分支**先行短
路返回**（本包函数不入 Deps，不能也不需校验）——本地路径零改动、零回归。从 receiver 精确 sealed 类型向上、
第一个 FQ 命中即「最近声明」= 对该精确类型对象动态派发的唯一目标。

- **边界**：**可限定（本地/imported）非泛型类** receiver（不再要求整类 sealed，见下 sealed override）+ 可限定
  非泛型定义类 + 非 abstract 目标。**任何解析不确定即回落 `VCallInstr`**（`ResolveSealedTarget` 返回 ""）——永不 miscall。
- **正确性铁律**：目标名错 = 静默调错。多保险：① 越界回落 VCall；② imported 定义类必过 Deps FQ 校验；
  ③ `--no-opt devirt` before/after 逐字节对拍（单源 IR 路径）+ z42c 自举不动点（z42c 自身大量本地/imported
  sealed 类被去虚化编译，gen1==gen2 覆盖全码库）；④ cross-zpkg e2e `sealed_devirt_imported`（跨包继承基链——
  正是它抓到上面的 TSIG 展平坑）。

### sealed override 去虚化（非 sealed 类上的 sealed 方法，extend-sealed-devirt-more）

整类未 sealed，但某方法标 `sealed override`（`sealed` 简写）→ 该**方法**不可再被 override。于是从「能看见
这个 sealed 方法」的 receiver 静态类型 `S` 起，该方法调用同样目标唯一：

- **判据**：沿 `S` 基链找到 declClass = 最近声明该方法的可限定类。因 walk 向 base 方向 → `S ≤ declClass`；
  运行期 `R ≤ S ≤ declClass`。declClass 上该方法 `sealed`（语义强制其下无人能 override）→ `R` 的最派生实现
  = declClass 的 → **唯一**。
- **实现**：入口 `SealedReceiverClass` 泛化为 `DevirtReceiverClass`（删「整类 sealed」要求，接纳任意可限定非泛型类）；
  `ResolveSealedTarget` 加 `classSealed` 参数，declClass 处门控 **`classSealed || ms.IsSealed`**（`MethodSymbol.IsSealed`
  由 #140 本地/跨包序列化）。整类 sealed → 恒真（逐字节复现原 add-sealed-devirt 行为）；非 sealed 类 → 仅方法
  sealed 才命中；皆非 → `""` 回落 VCall。
- **反例（必回落）**：`class Mid : Base { override int M(){} }`（**非** sealed override）——`Mid m` 可持 `Leaf : Mid`
  且 Leaf override M → 目标不唯一 → 门控失败 → VCall。多态保持正确。
- **e2e**：`src/tests/classes/sealed_override_devirt.z42`——sealed override 去虚化结果 == 虚派发、子类继承 sealed
  方法安全、非 sealed override 保持多态。

### 泛型 sealed 去虚化（$N 条件 arity-mangle，extend-sealed-devirt-more）

`sealed class Box<T>` 同样不可继承 → `Box<int>` receiver 运行期类型必是它自身（类型擦除后一份 `Box`）→ 方法
目标唯一。之前保守回落，本项接纳：

- **receiver 解包**：泛型实例 `Box<int>` 的静态类型是 `Z42InstantiatedType` → `DevirtReceiverClass` 解包 `.Def`
  拿到泛型定义类。
- **$N 条件 mangle**：泛型是**类型擦除**，方法一份发射；短名由 `_classShortName` 镜像 `IrGen._classIrShortName`
  ——泛型类**仅当同名多 arity 重载**（`Symbols.HasClass("Name$N")`）才用 `Name$N`，否则裸 `Name`。目标名
  `QualifyClass(_classShortName(ct))+"."+RegKey`、`ImportedClassNs` 查键、`TrackImportedClass` 都用它，逐字节匹配
  IrGen 发射。非泛型下 `_classShortName==Name` → 与 v1 逐字节等价（零回归）。
- **单测**：`test_generic_sealed_devirt`（单 arity → `call @Box.`）/ `test_generic_sealed_multiarity_devirt`
  （`Box`+`Box<T>` → `call @Box$1.`）/ `test_generic_nonsealed_stays_vcall`；e2e `sealed_generic_devirt.z42`
  （`Box<int>`/`Box<string>` 去虚化结果正确 + 非 sealed 泛型多态）。

## Deferred / Future Work

### sealed-devirt-future: 去虚化 v1 边界外

- **来源**：`add-sealed-devirt` v1（本地）+ `extend-sealed-devirt-imported`（imported）+ `extend-sealed-devirt-more`（sealed override / 泛型）。
- **已落地**：**本地非泛型**（`add-sealed-devirt`）；**imported sealed 类**（`extend-sealed-devirt-imported`）；
  **sealed override 方法**（非 sealed 类上的 sealed 方法）；**泛型 sealed 类**（`$N` 条件 arity-mangle）
  ——后两者见上「sealed override 去虚化」「泛型 sealed 去虚化」节（`extend-sealed-devirt-more`）。
- **剩余（回落 VCall，仍正确、只是不内联）**：非虚方法/接口 receiver/cast-unknown 链（既有守卫优先）；
  数据流型别精化（`if (x is T)` 后窄化）——仍按静态声明类型，不做流敏感分析。
