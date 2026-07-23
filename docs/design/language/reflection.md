# 反射（Reflection）

> 状态：MVP 落地（add-reflection-mvp, 2026-06-08）—— 只读元数据检视。
> **非泛型 `MethodInfo.Invoke` + `Type.GetType(fqn)` 已落地（add-method-invoke-non-generic, 0.3.12）**
> —— 见下「反射调用」。泛型方法 `Invoke` / `Activator.CreateInstance` / `MakeGenericType` 随运行期
> 泛型实例化在 0.4.x G / 0.5.x L3-R 落地。

z42 反射让程序在**运行时只读地检视类型**——字段、方法、基类、泛型实参。API 形态参照 C# `System.Type` / `System.Reflection`。

## 用法

入口是 `Object.GetType()`：

```z42
using Std;
using Std.Reflection;

class Point {
    public int X;
    public int Y;
    public int Dist2() { return this.X * this.X + this.Y * this.Y; }
}

void Demo() {
    Point p = new Point();
    Type t = p.GetType();

    // 类型身份
    Console.WriteLine(t.Name);        // "Point"
    Console.WriteLine(t.FullName);    // "<ns>.Point"
    Console.WriteLine(t.BaseType.Name); // "Object"

    // 字段（含继承的实例字段）
    foreach (FieldInfo f in t.GetFields()) {
        Console.WriteLine(f.Name + " : " + f.FieldType.Name);  // "X : int" ...
    }

    // 方法（含继承 / 虚方法）
    foreach (MethodInfo m in t.GetMethods()) {
        Console.WriteLine(m.Name + " -> " + m.ReturnType.Name);
        foreach (ParameterInfo pi in m.GetParameters()) {
            Console.WriteLine("  " + pi.Name + " : " + pi.ParameterType.Name);
        }
    }
}
```

### typeof(T)（编译期类型 → Type）

`typeof(T)` 求值为 `Std.Type`（与 `obj.GetType()` 同一类型身份），不再是字符串：

```z42
using Std;

class Point { public int X; public int Y; }

void Demo() {
    Type t1 = typeof(int);
    Console.WriteLine(t1.Name);              // "int"（i32→int 规范化）

    var tp = typeof(Point);
    Console.WriteLine(tp.Name);              // "Point"
    Console.WriteLine(tp.GetFields().Length); // 2 —— 带真句柄，成员可枚举

    Point p = new Point();
    Type viaGetType = p.GetType();
    // typeof 与 GetType 一致（同一类型身份）
    Console.WriteLine(tp.Name == viaGetType.Name); // true
}
```

**关键能力**：编译器在 `FunctionEmitter.VisitTypeof` 把目标类型解析成**限定名**（用户类经 `QualifyClassName` → `<ns>.Point`），运行时 `make_type_from_name` 据此命中主模块 `type_registry` → **真 `TypeDesc` 句柄**，故 `typeof(Point).GetFields()` 可枚举成员（不像早期 desugar-to-string 方案只有名字）。基础类型（`int`/`string`/…）→ synthetic Type（Name/FullName 有值）。

> **已修（2026-06-10，fix-chained-property-dispatch）**：`var x = obj.GetType(); x.Name` 现可直接用——`GetType()` 的返回类型已从 `Unknown` 升级为真实 `Std.Type` 类（Object stub 修复），故 `var` 推断为 `Type`、属性派发正常。typeof 的 `var tp = typeof(Point)` 一向正常。详见 Deferred `reflection-future-chained-property-dispatch`（已落地）。

### Type.GetProperties()（属性视图）

`GetProperties()` 返回该类型的属性（含继承），从 `get_<X>` / `set_<X>` 访问器约定派生——getter 使属性可读，setter 使其可写，同名 getter+setter 合并为一条：

```z42
using Std.Reflection;

class Account {
    public int Balance { get; set; }    // read-write
    public string Owner { get; }        // read-only
}

void Demo() {
    Type t = typeof(Account);
    foreach (PropertyInfo p in t.GetProperties()) {
        Type pt = p.PropertyType;        // 局部接收者（链式属性派发限制）
        Console.WriteLine(p.Name + " : " + pt.Name + " r=" + p.CanRead + " w=" + p.CanWrite);
    }
    // Balance : int r=true w=true
    // Owner   : string r=true w=false
}
```

- 访问器方法（`get_X` / `set_X`）**仍出现在 `GetMethods()`**（与 C# 一致）；`GetProperties()` 是叠加的属性视图。
- **`GetValue(obj)` / `SetValue(obj, value)`**（add-property-getvalue-setvalue，2026-07-15）：反射读写实例属性值——经隐藏槽 `__getterQualified` / `__setterQualified`（VM 在 `__type_properties` 派生时写入访问器限定名）走非泛型 invoke 通路调 `get_<X>` / `set_<X>`。只读属性 `SetValue` / 只写属性 `GetValue` 抛 `Std.Exception`；访问器内 `throw` 原类型传播（与 `MethodInfo.Invoke` 共用 `invoke_qualified` helper）。**无格式 bump、零编译器改动**。按**声明类**访问器调用（不做虚 override 派发，见 Deferred）；静态 / 索引器属性延后。基础类型 / 数组的 Type 返空属性集。

### API（MVP）

| 类型 | 命名空间 | 成员 |
|------|---------|------|
| `Type : MemberInfo` | `Std`（z42.core）| `Name`（继承自 MemberInfo）/ `FullName` / `BaseType` / `IsAbstract` / `IsSealed` / `IsValueType` / `IsRecord` / `IsClass` / `IsInterface` / `IsGenericType` / `IsGenericTypeDefinition` / `IsDelegate` / `IsPrimitive` / `IsArray` / `GetFields()` / `GetMethods()` / `GetMembers()` / `GetProperties()` / `GetGenericArguments()` / `GetGenericTypeDefinition()` / `GetInterfaces()` / `GetInterface(name)` / `IsAssignableFrom(Type)` / `GetElementType()` |
| `MemberInfo` | `Std.Reflection` | `Name`（`Type` / `FieldInfo` / `MethodInfo` / `PropertyInfo` 的基类）|
| `FieldInfo : MemberInfo` | `Std.Reflection` | `FieldType : Type` / `IsStatic` / `IsPublic` / `IsPrivate` / `GetCustomAttributes()` / `GetAttribute(Type)` |
| `MethodInfo : MemberInfo` | `Std.Reflection` | `ReturnType : Type` / `IsStatic` / `IsVirtual` / `IsAbstract` / `IsPublic` / `IsPrivate` / `GetParameters()` / **`Invoke(object, object[])`**（0.3.12，非泛型）|
| `ParameterInfo` | `Std.Reflection` | `Name`（SIGS 权威） / `ParameterType : Type` / `Position` / `IsOptional` / `IsParams` / `DefaultValue` / `GetCustomAttributes()` / `GetAttribute(Type)` |
| `PropertyInfo : MemberInfo` | `Std.Reflection` | `PropertyType : Type` / `CanRead` / `CanWrite` / `GetValue(obj)` / `SetValue(obj, value)` |

`FieldType` / `ReturnType` / `ParameterType` / `BaseType` 返回的都是 `Type` 对象（不是字符串），与 C# 一致。

`Type` 另有静态 **`Type.GetType(string fqn)`**（0.3.12）：完全限定名 → `Type`（未知返 null），补 `typeof`/`obj.GetType()` 的反向查询。

## 反射调用（add-method-invoke-non-generic, 0.3.12）

非泛型 `MethodInfo.Invoke(object obj, object[] args) → object`：

```z42
Type t = Type.GetType("Demo.Calc");
MethodInfo add = /* 从 t.GetMethods() 按名取 */;
object r = add.Invoke(null, new object[]{2, 3});   // 静态：obj=null；装箱实参进 object[]
int sum = (int)r;                                  // 5（拆箱返回值）

Calc c = new Calc();
object r2 = twice.Invoke(c, oneArg);               // 实例：obj=接收者（reg 0）
```

语义：
- **静态方法** obj 传 null（忽略）；**实例方法** obj 为接收者。`args`（`object[]`）按序映射形参——
  依赖装箱（[boxing.md](boxing.md)）：原始实参装入 `object[]`、调用时直接落参数寄存器。
- **返回值**：方法返回值（`void` → null）；拆箱由调用方 `(T)` 完成。
- **异常传播**：被调方法的 `throw` 以**原始异常类型**传播，调用方可 `try/catch` 精确匹配
  （非包装成 `Std.Exception`）。实现：builtin 经 `ctx.pending_thrown` 把原异常值透出，interp
  （`exec_call::builtin`）与 jit（`jit_builtin`）两路径都消费它。
- **实参个数不符** → 抛带信息异常。
- **实现**：`__method_invoke` builtin 复用普通调用的 `exec_function`（同一调用路径，零重复）。

**反射构造**：`Activator.CreateInstance(Type t) → object`（`Std.Reflection.Activator`，非泛型无参）——
反射 `new`：按 `<FQClass>.<SimpleName>`（含 `$0`）找无参 ctor 并执行（无显式 ctor 则默认字段分配即构造），
ctor `throw` 以原类型传播。供反射测试运行器实例化测试类。

```z42
object inst = Activator.CreateInstance(typeof(Counter));   // 反射 new（跑无参 ctor）
object r = bump.Invoke(inst, args);                        // 再反射调用其方法
```

非泛型限制：MethodInfo 不记类型实参；泛型方法 Invoke / `Activator.CreateInstance<T>` / `MakeGenericType`
+ **有参 `Activator.CreateInstance(Type, args)`**（构造重载决议）待运行期泛型实例化（0.4.x G / 0.5.x）。

## 实现原理

### 为什么所有反射类型都在 z42.core

`Object.GetType()` 返回 `Type` → `Type` 必须在 z42.core（否则 z42.core 依赖外部包）。`Type.GetFields()` 返回 `FieldInfo`、`FieldInfo.FieldType` 又返回 `Type` → 若 `FieldInfo` 在独立 `z42.reflection` 包，则 `z42.core ↔ z42.reflection` 循环依赖，包构建顺序无解。

解法照搬 .NET mscorlib：`System.Type` 与 `System.Reflection.*` 同在核心程序集、仅命名空间不同。z42 对应 `Std.Type` + `Std.Reflection.*` 同在 **z42.core**。独立 `z42.reflection` 包留给 0.5.x 的 `Method.Invoke` / `Activator`（它们可**单向**依赖 core）。

### 为什么不拆 Type / TypeInfo（统一模型，2026-06-09 定）

z42 用**单一 `Std.Type`** 同时承载类型身份（`Name`/`FullName`/`BaseType`/泛型实参）与成员检视（`GetFields`/`GetMethods`/`GetMembers`/attributes），**刻意不引入 C# 的 `Type` / `TypeInfo` 拆分**。

**C# 拆分的来龙去脉**：原始 .NET Framework（2002–）是统一 `Type`，无拆分。`TypeInfo` 2012（.NET 4.5）为 WinRT 受限 profile 引入，2016（.NET Core / Standard 1.x）为「模块化 corelib + AOT trimming（反射 pay-for-play）」推广强制——`type.GetTypeInfo()` 过桥。仪式负担招致反感，2017（.NET Standard 2.0 / Core 2.0）把反射 API 搬回 `Type`；`TypeInfo` 今天仅作 back-compat 残骸存活。**.NET 自己兜一圈又收敛回统一 `Type`。**

为什么 z42 直接用统一模型：

1. **无 back-compat 包袱**：绿地语言，没有理由背 `TypeInfo` 这个残骸；直接落在 .NET 收敛到的统一形态。
2. **拆分的技术驱动（trim 反射栈）在 z42 不成立**：z42 反射是 **native-delegated**——`Type` 只揣 `NativeData::TypeHandle`，"重"的枚举在 native builtin（`__type_fields` 等），**不在托管 `Type` 类**。要 trim 反射 = 注册不注册那些 builtin，与 `Type` 拆成几个类无关。拆分在 z42 省不下任何东西。
3. **MVP 是只读元数据**：`Invoke` / `Activator` / `MakeGenericType` 在 0.5.x 独立 `z42.reflection` 包——拆分本想隔离的"重"部分这里根本不在。
4. **拆分对 z42 是纯仪式负成本**：`typeof` / `GetType` 高频返回 `Type`；拆了要么处处 `GetTypeInfo()`，要么两边都挂成员 API（等于没拆 + 双份 native plumbing），还破坏 typeof→Type / `Type.GetCustomAttributes` / `MethodInfo` 等已落地 API。
5. **与 z42 一贯取舍一致**：[attributes.md](attributes.md) 已修了 C# attribute 5 处缺陷；不再把 C# 最被诟病的反射缺陷搬进来。

> 层级对齐已部分落地（2026-06-11 align-type-memberinfo-hierarchy）：`Std.Type : Std.Reflection.MemberInfo`——`Name` 由基类统一提供（消除 Type 旧 `[Native]` getter 与 MemberInfo 字段的分叉），`typeof(C) is MemberInfo` 为真。`Type` 仍留在 `Std` prelude（不迁 `Std.Reflection`，保 `typeof`/`GetType` 免 import 人体工学）。剩余：嵌套类型纳入 `GetMembers()`——见 Deferred。

### Type 如何连回真实元数据：NativeData::TypeHandle

每个堆对象（`ScriptObject`）持有 `Arc<TypeDesc>`（类身份）。`GetType()` 时 VM 手上就有这个 Arc，直接把它存进返回的 `Std.Type` 对象的 `NativeData::TypeHandle(Arc<TypeDesc>)`。反射 builtins（`__type_fields` / `__type_methods` / `__type_base` / `__type_generic_args` / `__type_members`）从该句柄读 `TypeDesc` 枚举成员。

- O(1)、跨 zpkg 安全（Arc 就是真身，无需按名/TypeId 二次查表）。
- 与既有 `NativeData::WeakRef` 同款模式。
- **基础类型 / 数组**（`int` / `T[]` / `string`）的 `GetType()` 返回**无句柄**的 `Std.Type`（`NativeData::None`）：`Name`/`FullName` 有值，但成员查询退化为空数组 / null —— builtins 见无句柄即 lenient 返回空，绝不 `bail!`。

### 成员元数据来源

| 反射数据 | 运行时来源 |
|---------|-----------|
| 字段（名 + 类型）| `TypeDesc.fields: Vec<FieldSlot{name, type_tag}>`（已含继承，base 在前）|
| 方法名 + 虚标志 | `TypeDesc.vtable: Vec<(simple, qualified)>`（虚/继承）+ `cold.own_methods`（声明的非虚）|
| 方法签名（参数类型 / 返回 / 静态）| 按 qualified 名经 `ctx.try_lookup_function` 拿 `Function { param_count, ret_type, is_static, cold.param_types }` —— **已加载在运行时**，无需新 wire 格式 |
| 基类 | `TypeDesc.base_name`（无则 `Std.Object`；`Std.Object` 自身 → null）|
| 泛型实参 | `TypeDesc.cold.type_args`（实例化泛型）|

Member / Type 对象**eager 填充**：每个 builtin 用 `ctx.try_lookup_type("Std.Reflection.FieldInfo")` 等取真实 z42 类 TypeDesc，按 `field_index` 名字写槽。

### 属性派生（无新元数据，add-reflection-properties）

`__type_properties` **不持久化任何 PropertyDesc 元数据**——属性纯运行期从 `get_<X>` / `set_<X>` 方法名约定派生（auto-property 降解为 field + `get_`/`set_` 方法，约定已是编译器既定事实，见 [attributes.md] 同源的 auto-property getter dispatch）。builtin 扫 `TypeDesc.vtable`（含继承，base-first）+ `cold.own_methods`，按前缀配对去重：`get_<X>`（0 逻辑参）记 getter，`set_<X>`（1 逻辑参）记 setter，同名合并；`PropertyType` 取 getter 返回类型（无 getter 时取 setter 参数类型），与 `MethodInfo` 签名同源（`Function.ret_type` / `cold.param_types`，已加载，无新 wire）。**故无 zbc 格式变更**——这是它能在 0.3.x 干净落地（不撞自举 zbc-writer 移植）的关键。逻辑参数 arity 不符（`get_X(args)` / `set_X` 多参）的方法按普通方法跳过，不误判为属性。

### 类修饰符标志（add-reflection-type-flags，zbc 1.12）

`Type.IsAbstract` / `Type.IsSealed` 由 zbc TYPE section 每类的 `flags: u8` 字节背书（bit0 abstract / bit1 sealed / bit2 struct / bit3 record）。编译期 `IrGen.EmitClassDesc` 从 `ClassDecl` 取修饰符，`ZbcWriter` 写在每类记录末尾（attr block 之后）；运行期 `read_type` 载入 `ClassDesc.class_flags` → `build_type_registry` → `TypeDesc.class_flags`（hot，1 字节）。builtin `__type_is_abstract` / `__type_is_sealed` 读对应 bit；handle-less Type（基础类型/数组）→ false。修饰符是**声明级**，不随继承传播。**一次 bump 捕获 4 位**：`IsValueType`（struct bit2）/ `IsRecord`（record bit3）已于 2026-06-10（add-reflection-value-record-flags）纯 stdlib + runtime 加 getter 兑现，**无再次格式 bump**——这正是 type-flags 一次写满 4 位的回报。`IsStatic` 未暴露——z42 无 `static class` 概念。

**类别谓词 IsClass / IsInterface（add-reflection-interface-class-predicates，zbc 1.19）**：flags 字节扩 **bit4 = interface**（bit5 预留 enum）。关键前置——**interface 此前完全不产 TYPE 条目**（`IrGen.Generate` 只迭代 `cu.Classes`），故 `typeof(IFoo)` 是 name-only synthetic、读不到任何元数据。本变更让 `IrGen.EmitInterfaceDesc` 为每个接口产**最小 ClassDesc**（identity + TypeParams + flags，无 base/字段/方法表，`IsAbstract=true`），接口遂解析到真句柄。语义：`IsInterface` = 有句柄且 bit4；`IsClass` = 有句柄且 `!struct && !interface`（记录是引用型 → IsClass true，对齐 C# record class）。handle-less（基元/数组/未来 enum）两者皆 false——z42 数组是 name-only synthetic，故 `typeof(int[]).IsClass == false`（与 C# 数组 IsClass 不一致，见 Deferred）。**配套根因修**：codegen `Z42TypeName` 此前不处理 `Z42InterfaceType` → 落 `ToString()` 产**未限定**名，与接口条目的 FQ 名（`QualifyName`）不匹配 → `make_type_from_name` 漏句柄。加 `Z42InterfaceType => QualifyClassName(Name)` 分支修复。

### 类静态字段反射（add-reflection-static-fields，zbc 1.13）

`Type.GetFields()` 在实例字段（base-first，含继承）之后追加该类的**静态字段**，每个 `FieldInfo` 带 `IsStatic`（实例 false / 静态 true）。根因处理：运行期原本只有 `VmContext.static_fields`（全局按名 key 的 Value 槽，**无 per-type 类型元数据**），故编译期把静态字段 (名, 类型) 持久化进 zbc TYPE section 的**独立静态字段块**（在 flags 字节之后，与实例字段块同形）。静态字段**独立于** `TypeDesc.fields`（实例热路径布局）——存于 `TypeDescCold.static_fields`（cold，反射专用，普通无静态字段的类不分配 cold）。`builtin_type_fields` 合并两者输出。**MVP 仅声明类自身静态字段**——静态字段非继承存储，不走实例字段的 cross-zpkg base-merge fixup；继承静态反射见 Deferred。

### 成员可见性反射（add-member-visibility，zbc 1.23 / unify-type-metadata P1-b）

`FieldInfo` / `MethodInfo` 各带 `IsPublic` / `IsPrivate`（bool），反映成员的声明修饰符。可见性以 **u8**（0=public / 1=private / 2=protected）持久化到两处：字段进 zbc **TYPE section** 每字段块（实例 + 静态，紧接该字段的 attr 块之后），方法进 **SIGS section** 每函数（`is_static` 字节之后）——两者都是 **non-gated**（每成员固定 +1 字节），因 IrGen 对每个成员都算得可见性（缺省 public，镜像 z42 AST `Visibility` 默认）。运行期 reader 把 TYPE 的可见性灌进 `FieldSlot`（实例）/ `FieldDesc`（静态）、把 SIGS 的灌进 `Function.visibility`（与 `is_static` 同源同路径）；`build_field_info` / `build_method_info` 据此设 `IsPublic = (vis==0)` / `IsPrivate = (vis==1)`。`protected` 两者皆 false（镜像 C# `IsFamily`）。

> **unify-type-metadata 语境**：本砖只把可见性**加进** TYPE/SIGS（superset 阶段），编译期仍并存 TSIG 的可见性字段；待 P3 从 TSIG/EXPT 收敛（z42c 改读 TYPE/SIGS）后删 TSIG，实现「运行期反射元数据 = 单一真相」。

### 方法修饰符反射（add-method-modifiers，zbc 1.24 / unify-type-metadata P1-c）

`MethodInfo` 带 `IsVirtual` / `IsAbstract`（bool），反映方法的声明修饰符。修饰符以 **u8
`method_flags`**（bit0=virtual / bit1=abstract）持久化到 zbc **SIGS section** 每函数（紧接
`visibility` 之后，non-gated 固定 +1 字节）。`static` **不**入此字节——由既有独立 `is_static`
字节表达（单一真相，不重复）。IrGen `_methodFlags(mods)`：`virtual`/`override`/`abstract`
任一 → bit0（三者皆虚派发，镜像 C# `MethodInfo.IsVirtual`）；`abstract` → 另置 bit1。运行期
reader 把 method_flags 灌进 `Function.method_flags`（与 `is_static`/`visibility` 同源同路径）；
`build_method_info` 设 `IsVirtual = (flags & bit0) || <vtable 回退>`、`IsAbstract = (flags & bit1)`。

> **`IsVirtual` 权威化**：此前 `IsVirtual` 由「方法是否来自 vtable 迭代」的运行期启发式给出——
> 现改为读声明 flag（vtable-presence 仅作 flag 缺失的合成函数回退）。绝大多数方法一致（vtable
> 成员 ≈ virtual/override 方法），差异在于「碰巧进 vtable 但非虚声明」的边界现在如实反映声明。

> **abstract 方法可观测（IrGen `_emitAbstractStub`）**：实例 `abstract` 方法无 body，此前 IrGen
> （`md.HasBody` 门）完全不 emit → 元数据里没有该方法 → 反射看不到、`IsAbstract` 观测不到 true。
> P1-c 为实例 abstract 方法发一个 **signature-only 死体桩**（`ret null`/`ret`）进 SIGS/FUNC——
> abstract 方法只被 override 经 vtable 派发，桩本身（抽象类 slot）是死代码；这样其 `method_flags`
> 带 abstract 位、进得了 `Function` → 反射 `GetMethods()` 列出它且 `IsAbstract == true`。**限实例
> abstract**（`static abstract` 接口成员如 `INumber` 是独立的静态抽象接口特性，不在此列）。z42c /
> stdlib 源无实例 abstract 方法 → 现有 zpkg 零字节漂移、自举不动点不受影响。

### 参数元数据反射（add-param-metadata，zbc 1.25 / unify-type-metadata P1-d）

`ParameterInfo` 带 `IsOptional` / `IsParams` / `DefaultValue`，且 `Name` **权威化**。SIGS 每函数
在 `method_flags` 后追加 `min_arg:u16`（必填逻辑参数个数——可选参数恒尾随，`IsOptional = pos >=
min_arg`）+ `params_from:u8`（varargs 逻辑 index，0xFF=无，`IsParams = pos == params_from`）；每
参数在 `param_type` 后追加 `name_str_idx:u32`（源名；this 槽 = `"this"`）+
`default_kind:u8 + payload`（0=无 / 1=null / 2=i64 / 3=f64bits / 4=bool / 5=str idx）。

- **Name 权威化**：此前从 DBUG 局部变量表猜（`reg == 参数索引`；无 debug 符号退化 `arg{n}`）——现
  SIGS name 优先（`resolve_func_sig` 判 SIGS names 齐全即用），DBUG 仅作回退。strip 后的 release
  包参数名照样可反射。
- **DefaultValue 字面量折叠**：IrGen `_fillParamMeta` 从 `Param.Default`（AST `Expr`）折出字面量
  （IntLit/FloatLit/BoolLit/StringLit/CharLit/null）；**非字面量**（常量表达式/enum 成员）→ kind 0，
  `DefaultValue == null` 但 `IsOptional` 仍 true（见 Deferred）。
- **口径**：min_arg/params_from/IsOptional/IsParams 均为**逻辑 position**（0-based，不含 this）；
  SIGS 每参数组（名/默认值）按 wire 索引（含 this 槽）对齐，反射侧 `start = is_static?0:1` 跳过。
- **运行期**：`Function.min_arg/params_from`（hot）+ `FunctionCold.param_names/param_defaults`
  （cold，与 param_types 同伴）；`build_method_info` 构造 ParameterInfo 时一并写槽。

### 跨包 impl 反射（add-crosspkg-impl-reflection，unify-type-metadata P1-e）

`impl TraitT for TypeA` 声明在包 B（TypeA 在包 A）时，方法**派发**一直可用（vcall 走 vtable +
func_index 按名解析），但此前 VM 不读 IMPL 段 → `typeof(TypeA).GetInterfaces()` 反射不到 B 加的
TraitT。现在 VM 读每个已加载 zpkg 的**现有 IMPL 段**（无格式变化）：

- **解析**：`read_zpkg_impl_pairs(raw)` 独立读 dir+STRS+IMPL，只取 `(target_fq, trait_fq)` 对——
  type_args 与方法签名跳过（派发已有机制，反射只需类型↔trait 关联；方法记录定长
  `17B + param_count×8` 跳过，镜像 `ZpkgWriterZ._writeMethod`）。
- **注册表**：`LazyLoader.impls: target_fq → [trait_fq]`（追加语义——不同包各自 impl 不同 trait
  合法）。lazy 加载的 zpkg 在 `load_zpkg_file` 合并；eager 加载（z42.core / 主 zpkg / 依赖闭包）
  经 `seed_lazy_loader_impls` 并入（镜像 `seed_types_for_lookup`）。
- **反射**：`builtin_type_interfaces`（GetInterfaces/GetInterface 共用）沿 base 链每类除声明接口外
  并入注册表中该类的 impl traits，进同一 BFS 传递闭包（trait 的基接口自动展开、统一去重）。
- **加载语义**：反射只见**已加载**包的 impl——包 B 未加载则其 impl 方法本也不可调，一致；不为
  反射强制加载全部 declared zpkg（按需加载哲学不变）。

> **unify-type-metadata 语境**：IMPL 段由「编译期专用」重定性为「统一元数据，z42c + VM 都读」
> （initiative D2）；P3 删 TSIG/EXPT 时 IMPL 保留。

### delegate 反射（add-delegate-metadata，zbc 1.26 / unify-type-metadata P1-e ②）

delegate 此前是**纯编译期函数型别名**（运行期实例 FuncRef/Closure，TYPE 无条目）。现在
**delegate-as-class**（D5，C# 式）：每个 `delegate` 声明（含泛型）emit 一条 TYPE 条目
（FQ 名 + `class_flags` **bit6=delegate** + TypeParams——泛型 tps 存 TYPE 条目，Invoke 按名
引用）+ 合成 `<FQ>.Invoke` **死体桩**进 SIGS/FUNC（实例、virtual、参数源拼写类型 + 源名 +
P1-d 全套参数元数据；真实 delegate 调用走 CallIndirect，桩永不被调）。

- `Type.IsDelegate`（`__type_is_delegate`，读 bit6）；签名经 `GetMethods()` 的 Invoke 反射
  （own_methods 前缀扫描自动含它，MethodInfo/ParameterInfo 全套面可用）。
- 反射入口：`Type.GetType("<fq>")`（运行期 registry）。**typeof(MyDelegate) 暂不支持**——
  delegate 名在 `ResolveTypeP` 必须继续解析为函数类型（改了会破坏 delegate 赋值检查），
  typeof 路径特判留 Deferred。
- **P3 前置**：TSIG 携带 delegate 定义（含泛型内建 Action/Func/Predicate）供跨包解析——删
  TSIG 前 delegate 必须能从 TYPE（名/tps）+ SIGS（Invoke 签名）重建，本砖即该重建的家。

### 类型名规范化

VM 内部有两套 tag 词汇：字段槽用 `"int"`/`"long"`，函数签名用 `"i32"`/`"i64"`/`"str"`。反射在 `make_type_from_name` 处统一映射到 C# 风格别名（`i32→int` / `i64→long` / `str→string` / …），让 `FieldType.Name` 与 `ReturnType.Name` 对同一类型给出一致结果。用户类名（非基础 tag）原样透传。

## 数组元素类型（IsArray / GetElementType）

数组在 z42 运行期**不类型擦除**——`T[]` 的值携带元素类型，反射可还原：

```z42
var t = typeof(int[]);
bool isArr = t.IsArray;                 // true
string elem = t.GetElementType().Name;  // "int"

typeof(int).IsArray;                    // false
typeof(int).GetElementType();           // null（非数组）

int[] xs = new int[3];
xs.GetType().GetElementType().Name;     // "int"（运行期值非擦除）
```

**实现原理（根因：不擦除）**：数组运行期表示从不带元素类型的 `GcRef<Vec<Value>>` 升级为 `GcRef<ArrayObj>`，`ArrayObj { element_type: Arc<str>, elems: Vec<Value> }`——`ArrayObj` 对 `elems` 实现 `Deref`/`DerefMut`/`Index`，故绝大多数数组消费点（GC trace / 索引 / 长度 / clone）改动透明。元素类型 FQ 名从编译期一路携带到运行期：

- **编译期**：`ArrayNew` / `ArrayNewLit` 在 wire 上追加 `element_type: u32`（STRS idx，zbc 1.16）。codegen 用 `Z42TypeName(elemType)` 产 FQ 名（`int` / `geometry.Point` / `int[]`）——`IrType` 是粗枚举（丢类名），故元素类型名以字符串 thread。`typeof(T[])` 经同一 `Z42TypeName` emit `<elem>[]`。
- **运行期**：`make_type_from_name` 用 `name.strip_suffix("[]")` 识别数组 → 合成 `IsArray=true` + `__elementName` 槽的 `Std.Type`；`GetElementType()`（`__type_element` builtin）读 `__elementName` 递归 `make_type_from_name`（空/非数组返 null）。`obj.GetType()` 对 `Value::Array` 用其 `element_type` 合成 `<elem>[]` Type，与 `typeof` 一致。

**设计选型**：用 unified `Std.Type` + `IsArray`/`GetElementType()`（与既有 `GetGenericArguments()` 同风格），不引 z42-visible `ArrayType`/`GenericType` 子类——元素类型背在运行期值与 `Std.Type` 槽上，无需类型层级。jagged/多维数组 `typeof` 语法暂不支持（见 Deferred）。

## 实现的接口（GetInterfaces）

`Type.GetInterfaces()` 反射出一个类实现的接口——**本类直接声明的 + 沿继承链从基类继承的**（按名 dedup）：

```z42
class Base : IFoo {}
class Derived : Base, IBar {}

var ifaces = typeof(Derived).GetInterfaces();  // 含 IBar（own）+ IFoo（继承自 Base）
foreach (var i in ifaces) { var n = i.Name; }  // "IBar" / "IFoo"

typeof(NoIface).GetInterfaces();               // 空数组（非 null）
obj.GetType().GetInterfaces();                 // 与 typeof 一致
```

**实现原理**：接口名从编译期一路携带到运行期。

- **编译期**：codegen `EmitClassDesc` 从 `ClassDecl.Interfaces`（`List<TypeExpr>`）取每个接口的 bare 名（`NamedType.Name` / `GenericType.Name` 丢类型参数），存进 `IrClassDesc.Interfaces`。`ZbcWriter.BuildTypeSection` 把它写进 TYPE section 每类记录尾部的**接口块**（`interface_count: u16` + `name str idx[]`，zbc 1.17，紧随静态字段块）；`ZbcReader.ReadTypeSection` 对称读回（ReadWriteRoundTrip parity）。
- **运行期**：`read_type` 读接口块 → `ClassDesc.interfaces` → `TypeDescCold.interfaces`（cold，反射专用，无接口的类不付成本）。`Type.GetInterfaces()`（`__type_interfaces` builtin）沿 `base_name` 链聚合各祖先类的 `interfaces()`（most-derived first，按名 dedup），每名 `make_type_from_name` 还原为 `Std.Type`。

**设计选型**：wire 只持久化**每类自身声明**的接口（最小格式成本），继承接口由运行期 base-walk 聚合——镜像 `inherited-static-fields` 已验证的模式，零额外格式成本拿到 C# `GetInterfaces()` 默认含继承的语义。

> **2026-06-16 add-reflection-assignable-from（zbc 1.20）**：接口块从 bare 名（`IShape`）改存 **FQ 名**（`Demo.IShape`，编译期 `QualifyClassName`）。因接口现产 TYPE 条目（add-reflection-interface-class-predicates），`make_type_from_name(FQ)` 命中接口 TypeDesc → `GetInterfaces()` 返回**真接口句柄**（`IsInterface`/`FullName` 可用，不再 name-only）；接口身份按 FQ 名 robust 比较（修跨命名空间同名歧义）。

> **2026-06-17 add-reflection-transitive-interfaces**：`interface IBar : IFoo` 的基接口此前在 parser 就被丢弃（"Skip base interfaces"）；现 `InterfaceDecl.BaseInterfaces` 捕获 → `EmitInterfaceDesc` 写进**接口自身的接口块**（复用 #2/#3 结构，无 wire 新字段、无格式 bump，仅接口条目接口块空→非空）。运行期 `__type_interfaces` 对每个接口 **BFS 展开其基接口**（传递闭包）；`is`/`as`/`IsAssignableFrom`（`is_subclass_or_eq_td` interp + `is_subclass_or_eq` JIT）命中接口后递归其基接口。`class C : IB`、`IB : IA` → `typeof(C).GetInterfaces()` 含 IA、`c is IA` true、`typeof(IA).IsAssignableFrom(typeof(C))` true。

## 赋值兼容：IsAssignableFrom + 接口 is/as

`Type.IsAssignableFrom(Type c)` 是反射版的 `is`/赋值关系——`c` 的实例能否赋给 `this` 类型的变量：

```z42
typeof(Animal).IsAssignableFrom(typeof(Dog));   // true —— Dog 派生自 Animal（接收者是更宽类型）
typeof(Dog).IsAssignableFrom(typeof(Animal));   // false
typeof(IShape).IsAssignableFrom(typeof(Circle)); // true —— Circle 实现 IShape
typeof(C).IsAssignableFrom(typeof(C));           // true（自反）
typeof(C).IsAssignableFrom(null);                // false

typeof(Circle).GetInterface("IShape");           // IShape 的 Type（按名查，未命中返 null）

Circle c = new Circle();
bool ok = c is IShape;                            // true —— 接口 is/as 现已生效
```

**实现原理**：`IsAssignableFrom`（`__type_is_assignable_from` builtin）**复用 VM 权威的 `is_subclass_or_eq_td`**——比较真实 `TypeDesc` 的 FQ 名（`c` 的 base 链 + 各层接口），**不碰合成 Type 的字符串**。`GetInterface(string)` 是纯 z42（遍历 `GetInterfaces()` 按名匹配，输入即名字）。

**根因修：接口的 `is`/`as` 此前不工作**。VM 的 `is_subclass_or_eq_td`（interp）/ `is_subclass_or_eq`（JIT）此前只走 `base_name` 类继承链，**不查接口** → `circle is IShape` 返回 false。本变更让两者 base 链每层额外比较声明的接口（FQ 名）→ `is`/`as`/`IsAssignableFrom` 对接口一致生效。**传递接口**（接口继承接口）仍不覆盖。

## 泛型/基元谓词（IsGenericType / IsPrimitive）

两个布尔谓词，运行期从**已加载的类型元数据 + 类型名**派生，**无 wire / 格式变更**：

```z42
class Box<T> { public T Value; }
class Plain { public int X; }

typeof(Box<int>).IsGenericType;   // true —— 该类型有类型参数
typeof(Plain).IsGenericType;      // false
typeof(int).IsGenericType;        // false（基元非泛型）

typeof(int).IsPrimitive;          // true
typeof(bool).IsPrimitive;         // true
typeof(char).IsPrimitive;         // true
typeof(double).IsPrimitive;       // true
typeof(string).IsPrimitive;       // false —— string 非基元（对齐 C#）
typeof(Plain).IsPrimitive;        // false
```

**实现原理**：

- **`IsGenericType`**（`__type_is_generic` builtin）：读被反射类型 `TypeDesc` 的 `type_params()` / `type_args()`，任一非空即 true。数据源与既有 `GetGenericArguments()` 同——零额外存储。
- **`IsPrimitive`**（`__type_is_primitive` builtin）：基元 `Type` 是 name-only（无 `TypeDesc` handle），故直接读 `build_type` 写入的 `Name` / `__fullName` 槽，比对基元名集合 `is_primitive_type_name`——含源关键字形（`int`/`bool`/`char`/`double`…，反射已归一 `i32→int`）与其 BCL `Std.*` struct 名（`Std.Int32`/`Std.Boolean`…）。`string` **不算**基元（与 C# `typeof(string).IsPrimitive == false` 一致）。

**设计选型**：纯运行期派生（不持久化任何新元数据），延续 properties / inherited-static-fields / value-record-flags 的"零格式成本"模式——比需 wire bump 的 type-flags / static-fields 增量更轻。

## 构造型泛型：GetGenericArguments / IsGenericTypeDefinition / GetGenericTypeDefinition

`typeof(Box<int>)` 现在**携带实例化 type args**（add-reflection-generic-type-definition，zbc 1.18），故能区分**构造型** `Box<int>` 与**开放定义** `Box<>`：

```z42
class Box<T> { public T Value; }

Type bi = typeof(Box<int>);
bi.IsGenericType;                 // true
bi.IsGenericTypeDefinition;       // false —— 已构造（带 type args）
Type[] args = bi.GetGenericArguments();   // [typeof(int)]（此前返回空数组的 bug 已修）

Type def = bi.GetGenericTypeDefinition();  // Box<> 开放定义
def.IsGenericTypeDefinition;      // true
def.GetGenericArguments();        // 空（定义无实例化 args）

typeof(Plain).GetGenericTypeDefinition();  // 抛 InvalidOperationException（非泛型）
```

**实现原理**：根因修复——此前 `typeof(Box<int>)` 在 codegen 处就丢了 type args（`Z42TypeName(InstantiatedType)` 只取定义名），运行期解析到定义 `TypeDesc`（其 `type_args` 永远空）。

- **编译期**：新增 `Typeof` IR 指令（opcode `0x73`），携 `TypeName`（定义 FQ 名）+ 结构化 `TypeArgs`（实例化 arg FQ 名列表，镜像 `ObjNew` 的 count + STRS 索引编码）。所有 `typeof(...)` 统一走它，移除旧 `__typeof` builtin——type args 是编译期类型元数据，编码为指令字段比 materialize 成 `ConstStr` 运行期值更正确。zbc 1.17→1.18 / zpkg 0.19→0.20。
- **运行期**（interp + jit）：`make_constructed_type` 解析定义型 base，把实例化 args 解析为 `Std.Type[]` 挂到 base 对象的 `__typeArgs` 槽（镜像数组 `__elementName` 先例）。`IsGenericTypeDefinition` = 泛型且 `__typeArgs` 空（未构造）；`GetGenericArguments()` 优先读 `__typeArgs` 槽（回落 `TypeDesc.type_args`）；`GetGenericTypeDefinition()` 返回不带 `__typeArgs` 的定义型 handle（非泛型抛 `Std.Exception`）。**实例侧统一**（2026-06-16 add-reflection-instance-generic-args）：`new Box<int>()` 的 `obj.GetType()`（`__obj_get_type`）对泛型实例同样调 `make_constructed_type`，把 `ScriptObject.type_args` 挂 `__typeArgs` 槽——`obj.GetType()` 与 `typeof` 走同一构造型表示。

**设计选型**：**顶层**构造泛型实参选结构化 wire 编码（`Typeof` opcode 携 `string[]` 实参槽）而非把整串拼进类型名——顶层无歧义、与 `ObjNew` 一致。**嵌套层**（`Box<Pair<int,string>>` 的内层 `Pair<int,string>`）由 **add-reflection-nested-generic-args（2026-07-23）** 以**带尖括号的实参串**承载（`_typeofArgName` 递归产 `"Pair<int,string>"`，塞进现有 `string[]` 槽），运行期 `make_type_from_name` 按括号深度递归解析 → `make_constructed_type` 挂 `__typeArgs`，任意深度无损。**无 zbc/zpkg 格式 bump、TypeofInstr 接口不变**——嵌套层用括号串（而非扩展 wire 成递归 tree）是为避开自举「z42c 新用 z42.ir API 晚一 nightly」纪律（见 Deferred `reflection-future-nested-generic-args` 的 A-vs-B 权衡）。括号串匹配无歧义、非启发式，仅嵌套层，顶层仍结构化。

## 已知限制：链式属性访问需先赋值给局部变量

实测（add-reflection-mvp 验证）发现一个**编译器成员解析限制**（非反射本身）：extern 属性（`Type.Name`/`FullName`/`BaseType`，以及 `FieldInfo.FieldType`/`MethodInfo.ReturnType` 这类 `Type` 字段上的 `.Name`）**只在接收者是局部变量时才正确派发 getter**；接收者是方法调用结果或字段访问结果（链式）时，getter 不被调用，返回 null。

```z42
// ✗ 链式：属性 getter 不派发 → null
var n = obj.GetType().BaseType.Name;
foreach (var f in t.GetFields()) { var x = f.FieldType.Name; }  // f.FieldType.Name → null

// ✓ 先赋值给局部变量
Type t = obj.GetType();
Type bt = t.BaseType;            // 局部接收者 → 派发
string n = bt.Name;
foreach (var f in t.GetFields()) {
    Type ft = f.FieldType;       // 局部
    string x = ft.Name;          // ✓
}
```

extern **方法**（`GetFields()`/`GetMethods()`/`GetGenericArguments()`）不受影响——它们在任意接收者（含链式）上都正确派发（late-bound VCall）。根因是编译器对子表达式上的属性成员解析不完整；修复属编译器改动（与 0.3.x 进行中的 `port-z42c-*` 自举移植冲突），记入下方 Deferred。z42.core 的反射 `[Test]` 用例据此用局部变量写法。

## Deferred / Future Work

### ~~reflection-future-chained-property-dispatch~~ — 已落地（2026-06-10）
- **状态**：链式 getter 派发 `obj.GetType().BaseType.Name` 已落地 2026-06-10（fix-chained-property-dispatch）。`SymbolCollector` 预注册的 Object stub 的 `GetType()` 返回类型从写死 `Z42Type.Unknown` 改为从 `_classes` 取真实 `Std.Type` 类（`MergeImported` 在 `CollectClasses` 前已注入，短名 key `"Type"` 此刻可用）。dotnet test 1555/1555（含新 `src/tests/types/chained_property.z42` + 字节比对 fixture 无漂移）。
- **根因更正（插桩证伪了原 two-part 判断）**：原文档记的 P1+P2 两处降级，实测**只剩 P1**：
  1. **P1（真根因）= Object stub 的 `GetType` 返回 `Z42Type.Unknown`**（[SymbolCollector.Classes.cs](../../../src/compiler/z42.Semantics/TypeCheck/SymbolCollector.Classes.cs)）。`obj.GetType()` 类型为 Unknown → 链式 `.BaseType` / `.Name` 命中 [TypeChecker.Exprs.Members.cs](../../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Exprs.Members.cs):174 静默 fallback（当字段读 emit）→ 运行期 `FieldGet on Null`。修法 = 跨阶段降级 fixup（philosophy.md「在源头产出正确类型」）：stub 构建时即从 `_classes["Type"]` 取真实类。
  2. **P2（已被 fix-fqn-class-resolution 修掉，不复存在）**：原判「导入 getter 返回 FQN 名经 ResolveTypeName 降级 PrimType」。插桩确认 `t.BaseType.Name`（typeof 形式）**零 fallback、直接通过**——`fix-fqn-class-resolution`（归档 2026-06-09，namespace-aware ResolveMemberType）已把 getter 返回类型正确解析为 Std.Type 类。故本次**只需修 P1**。
- **"4th layer" 证伪**：attempt 3 称 `_classes` 无 `"Type"`/`"Std.Type"` key——插桩在 stub 构建点确认 `_classes["Type"]` **存在**（短名 key，name=Type）。前序 3 次失败的前提判断有误（查错 key / 错时机），非真有第 4 层。
- **关键调试教训（写给后人）**：xunit 吞 in-process `Console.Error`；golden 测的编译期插桩须写**临时文件**（`File.AppendAllText`）而非 stderr，否则只能看到 VM 子进程 stderr。standalone `--dump-bound` 不可信（与 workspace Std.Type 解析不同），复现须走 workspace golden（`src/tests/types/` 经 GoldenTests）。
- **副作用**：`var x = obj.GetType()` 现推断 `x: Type`（此前 Unknown）——顺带改善 [reflection-future-gettype-var-inference](#reflection-future-gettype-var-inference)（`var x = obj.GetType(); x.Name` 现可直接用）。

### ~~reflection-future-type-memberinfo-hierarchy~~ — 已落地（2026-06-11，层级对齐部分）
- **状态**：`Std.Type : Std.Reflection.MemberInfo` 已落地 2026-06-11（align-type-memberinfo-hierarchy）。`typeof(C) is MemberInfo` 为真；`Name` 由 `MemberInfo` 基类字段统一提供（移除 Type 的 `[Native("__type_name")]` getter + `__type_name` builtin；`build_type` 把简单名写入继承的 `Name` 槽，同 FieldInfo/MethodInfo）。`__name`/`__fullName` 字段 + `FullName` native getter 保留（低层 golden / z42.test 兼容；MemberInfo 无 FullName）。
- **实现要点（更正原前置依赖判断）**：① **无需跨命名空间限定基类**——base 解析是全局短名 `_classes.TryGetValue`（SymbolCollector.Classes.cs:370），`class Type : MemberInfo`（短名）直接命中同包 z42.core 的 MemberInfo；② **无 `TypeDesc` 布局 / zbc 格式改动**——TypeDesc 已支持 `base_name`(FQN) + 继承字段 cross-zpkg fixup，纯 stdlib 重编 + runtime `build_type` 调整；③ `Type.BaseType` 语义不变（反射被反射类型的基类，与 Type-类自身 `MemberInfo` 基类无关）。dotnet GoldenTests 1557/1557 + `type_is_memberinfo.z42` e2e + cargo 759+21。
- **剩余**：嵌套类型纳入 `GetMembers()` / `GetNestedTypes()`——见 `reflection-future-nested-types`（层级地基已就位）。

### ~~reflection-future-type-flags~~ — 已落地（2026-06-10）
- **状态**：`Type.IsAbstract` / `Type.IsSealed` 已落地 2026-06-10（add-reflection-type-flags）。zbc TYPE section 每类追加 `flags: u8`（zbc 1.12 / zpkg 0.14），运行期载入 `TypeDesc.class_flags`。见上文「Type.IsAbstract / IsSealed」+「类修饰符标志」。
- **剩余**：`IsStatic` —— z42 无 `static class` 概念，不暴露（bit 位预留）。
- **后续**：`IsValueType`（struct）/ `IsRecord` 已落地 2026-06-10（add-reflection-value-record-flags）——纯 stdlib + runtime 读 wire 已捕获的 struct/record 位，**无格式 bump**，兑现 type-flags 的预留设计。

### ~~reflection-future-properties~~ — 已落地（2026-06-09）
- **状态**：`Type.GetProperties()` + `Std.Reflection.PropertyInfo`（`PropertyType` / `CanRead` / `CanWrite`）已落地 2026-06-09（add-reflection-properties）。从 `get_`/`set_` 方法约定**运行期派生**（选了 Deferred 里"从方法名推导"的方案而非持久化元数据——零 zbc 格式变更，不撞自举 port）。见上文「Type.GetProperties()」+「属性派生」。
- **剩余**：~~`GetValue`/`SetValue`~~ —— 已落地 2026-07-15（add-property-getvalue-setvalue，复用非泛型 invoke 通路，无格式 bump；虚 override 派发 / 静态属性 / 索引器 延后）；~~properties 纳入 `GetMembers()`~~ —— 已落地 2026-07-16（members-include-properties，`__type_members` 追加 `__type_properties`，纯运行期无格式 bump；get_/set_ 访问器仍在 methods 切片，与 C# 一致）；~~隐藏 auto-property backing field~~ —— 已落地 2026-07-16（hide-autoprop-backing-field，`__type_fields` 过滤 `__prop_*` 合成后备字段，纯运行期；对齐 C# 隐藏 `<Name>k__BackingField`——属性经 GetProperties 可见）。

### ~~reflection-future-gettype-var-inference~~ — 已落地（2026-06-10）
- **状态**：`var x = obj.GetType(); x.Name` 已落地 2026-06-10（fix-chained-property-dispatch）。原判「根因在反射导入签名解析」**有误**：真根因 = `SymbolCollector` 预注册的 Object stub 的 `GetType()` 返回类型写死 `Z42Type.Unknown`（非导入签名解析）。改为从 `_classes["Type"]` 取真实 `Std.Type` 类后，`var` 推断为 `Type`、`x.Name` 正常。与 `reflection-future-chained-property-dispatch` 同一处修复。

### ~~reflection-future-element-typed-array~~ — 已落地（2026-06-12）
- **状态**：`Type.IsArray` / `Type.GetElementType()` + 非擦除 `arr.GetType()` 已落地 2026-06-12（add-reflection-array-element-type，zbc 1.16 / zpkg 0.18）。**根因修复（不类型擦除）**：数组运行期表示从 `GcRef<Vec<Value>>` 升级为 `GcRef<ArrayObj>`，`ArrayObj { element_type: Arc<str>, elems: Vec<Value> }`——元素类型 FQ 名随值携带。编译期 `ArrayNew` / `ArrayNewLit` 在 wire 上追加 `element_type: u32`（STRS idx），由 codegen `Z42TypeName(elemType)` 产 FQ 名（`int` / `geometry.Point` / `int[]`）。`make_type_from_name` 用 `name.strip_suffix("[]")` 识别数组类型 → `IsArray=true` + `__elementName` 槽；`Type.GetElementType()`（`__type_element` builtin）读槽递归 `make_type_from_name`（非数组返 null）。`obj.GetType()` 对 `Value::Array` 用其 `element_type` 合成 `<elem>[]` Type。
- **设计**：选 unified `Std.Type` + `IsArray`/`GetElementType()`（与既有 `GetGenericArguments()` 一致），不引 z42-visible `ArrayType`/`GenericType` 子类。`ArrayObj` 用 `Deref`/`Index` to `elems` 把 ~128 数组消费点的改动面收敛（多数经 Deref 透明不变）。
- **验证**：dotnet 1561/1561 + `array_element_type.z42` e2e（typeof(int[])/字段/字面量/`arr.GetType()` 非擦除/空数组/用户类元素/非数组返 null，interp+jit）+ cargo lib 807。
- **剩余**：jagged 数组类型 `int[][]` 的 `typeof` 语法暂不支持（type 解析器不接受嵌套 `[]`）；多维数组同。z42c 自举 writer 同步 **持续跟进**（2026-06-16 port-z42c-self-compile：`ArrayNew` element_type FQ 名已镜像，z42c.core FULL-FILE byte-identical；zbc 1.17/1.18/**1.19** 版本号 + 1.19 接口最小 TYPE 条目 emit 随之同步）——`xtask test compiler` byte-identical gate 持续全绿，z42c.core @ 1.19/0.21 仍逐字节一致。

### ~~reflection-future-parameter-names~~ — 已落地（2026-06-11）
- **状态**：`ParameterInfo.Name` 返回真实源参数名已落地 2026-06-11（add-reflection-parameter-names）。`resolve_func_sig` 从 `Function` 的 debug local-vars（`reg == 参数索引`，参数入口占寄存器 `0..param_count`）取名；`build_method_info` 用之，**无 debug symbols 时回落 `arg{n}`**。纯 runtime（读已有 DBUG 段），**无格式 bump**。dotnet GoldenTests 1560/1560 + `parameter_names.z42` e2e（`Add(int alpha, int beta)` → `ps[0].Name=="alpha"` / `ps[1].Name=="beta"`）+ cargo 764+21。
- **剩余**：无 debug symbols（strip 构建）时仍回落 `arg{n}`——这是设计预期（名字只在 DBUG 段）。

### ~~reflection-future-static-fields~~ — 已落地（2026-06-10）
- **状态**：`GetFields()` 含静态字段 + `FieldInfo.IsStatic` 已落地 2026-06-10（add-reflection-static-fields）。静态字段元数据持久化进 zbc TYPE section 静态字段块（zbc 1.13 / zpkg 0.15），运行期载入 `TypeDescCold.static_fields`，`GetFields()` 在实例字段后追加（IsStatic=true）。见上文「类静态字段反射」。
- **剩余**：~~继承的静态字段~~ —— 已落地 2026-06-11（add-reflection-inherited-static-fields，运行期 base-walk）。见 `reflection-future-inherited-static-fields`。

### ~~reflection-future-inherited-static-fields~~ — 已落地（2026-06-11）
- **状态**：`GetFields()` 含继承静态字段已落地 2026-06-11（add-reflection-inherited-static-fields）。`builtin_type_fields` 在实例字段后沿 `TypeDesc.base_name` base 链聚合各祖先类的 `static_fields()`（most-derived 优先，按名 dedup——derived `new`-式遮蔽胜），对齐 C# `GetFields()` 默认含继承公共静态。每个 `FieldInfo.__qualified` 用**声明类**名（attribute 解析定位正确类）。纯 runtime base-walk（`ctx.module().type_registry` → `try_lookup_type` 跨 zpkg 懒加载），**无格式 bump**。dotnet GoldenTests 1559/1559 + `inherited_static_fields.z42` e2e + cargo 764+21。
- **实现要点**：实例字段已含继承（cross-zpkg fixup 把祖先实例字段 base-first 合进 `td.fields`）；唯静态字段 per-声明类存储、不走该 fixup，故需运行期 base-walk。

### ~~reflection-future-get-interfaces~~ — 已落地（2026-06-14）
- **状态**：`Type.GetInterfaces()`（含继承接口）已落地 2026-06-14（add-reflection-get-interfaces，zbc 1.17 / zpkg 0.19）。TYPE section 每类追加接口块（`interface_count` u16 + name str idx[]），运行期 `TypeDescCold.interfaces`，`__type_interfaces` builtin 沿 base 链聚合 + 按名 dedup → name-only `Std.Type[]`。见上文「实现的接口（GetInterfaces）」。dotnet 1562/1562 + `get_interfaces.z42` e2e（单/多/继承/无接口/obj.GetType 一致；interp+jit）+ cargo lib 807 + xtask vm 356。
- **剩余**：见下方 `reflection-future-transitive-interfaces` + `reflection-future-get-interface-byname`。

### ~~reflection-future-transitive-interfaces~~ — 已落地（2026-06-17）
- **状态**：传递接口闭包已落地 2026-06-17（add-reflection-transitive-interfaces）。`interface IBar : IFoo` 的基接口经 `InterfaceDecl.BaseInterfaces`（parser 由"Skip"改"捕获"）→ 接口的 TYPE 条目接口块（FQ 名，复用 #2/#3 结构，**无格式 bump**）。`__type_interfaces` BFS 传递展开；`is_subclass_or_eq_td`（interp）/ `is_subclass_or_eq`（JIT）命中接口后递归其基接口。`GetInterfaces`/`is`/`as`/`IsAssignableFrom` 含间接接口。见上文「实现的接口」末尾。
- **剩余**：泛型接口 base 的 T 替换（`IList<T> : ICollection<T>`）；继承接口的方法纳入 `GetMethods()`。延后。

### ~~reflection-future-get-interface-byname~~ — 已落地（2026-06-16）
- **状态**：`Type.GetInterface(string)` + `Type.IsAssignableFrom(Type)` 已落地 2026-06-16（add-reflection-assignable-from，zbc 1.20 / zpkg 0.22）。**真实方案**：IsAssignableFrom 复用 VM 权威 `is_subclass_or_eq_td`（真实 TypeDesc FQ 名比较，非合成 Type 字符串）；接口块改存 FQ 名 → `GetInterfaces()` 返真句柄 + 接口身份 robust。**根因修**：扩展 `is_subclass_or_eq_td`（interp）/ `is_subclass_or_eq`（JIT）base 链每层查接口 → `x is IShape`/`as` 对接口生效（此前 false）。见上文「赋值兼容：IsAssignableFrom + 接口 is/as」。
- **剩余**：大小写不敏感 GetInterface / 泛型变体 / handle-less 装箱语义（`object.IsAssignableFrom(int)`）/ 传递接口 延后。

### ~~reflection-future-generic-type-definition~~ — 已落地（2026-06-16）
- **状态**：`Type.IsGenericTypeDefinition` / `GetGenericTypeDefinition()` + 修复 `GetGenericArguments()`-on-typeof 已落地 2026-06-16（add-reflection-generic-type-definition，zbc 1.18 / zpkg 0.20）。新 `Typeof` opcode 携结构化 type args；见上文「构造型泛型」。
- **剩余**：见 `reflection-future-nested-generic-args` + `reflection-future-instance-generic-args`。

### ~~reflection-future-nested-generic-args~~ — 已落地（2026-07-23）
- **状态**：嵌套泛型 `typeof(Box<Pair<int,string>>).GetGenericArguments()[0].GetGenericArguments()` 返 `[int,string]` 已落地 2026-07-23（add-reflection-nested-generic-args，**无格式 bump**）。**根因修在产出端 + runtime 递归解析（方案 A）**：此前 z42c `_typeofName` 把嵌套 instantiated 实参压成**裸定义名 `"Pair"`**（`<int,string>` 在发射时就丢了——比 doc 原记的「扁平串 `"Pair<int,string>"`」更早）。修法：新增 `_typeofArgName`，构造泛型实参递归产**带尖括号的完整名** `Pair<int,string>`（`Box<Pair<int,string>>` 任意深度），塞进 `TypeofInstr` 现有 `string[]` 实参槽——**wire 布局不变、TypeofInstr 签名不变**；运行期 `make_type_from_name` 检测 `<...>` → 按括号深度拆 base + 顶层 args（`split_generic_args`）→ `make_constructed_type`，其逐 arg 解析再回到 `make_type_from_name` → 天然递归到任意深度，每层挂 `__typeArgs`。见上文「构造型泛型」。
- **为何选 A 而非结构化递归 wire（B）**：B（`Typeof` opcode 携递归 `TypeNode` 树）须改 `TypeofInstr` 的 z42c↔z42.ir 接口 + 格式 bump → 撞自举「z42c 新用 z42.ir API 要晚一个 nightly」纪律（axis ③/④）+ CI 两代自举需先重建 z42.ir。A 保持 z42c↔z42.ir 接口不变（实参仍 `string[]`，仅串内容带括号）、runtime 侧（Rust，无自举约束）解析括号，单变更干净落地。顶层实参仍走结构化 `string[]` 槽，仅**嵌套层**用括号串（匹配无歧义、非启发式）。
- **剩余**：~~实例路径嵌套（`new Box<Pair<int,string>>()` 的 `obj.GetType()`）~~ —— 已闭环 2026-07-23（add-reflection-instance-nested-generic-args）：`ObjNew` 实参改用与 typeof 同款的 `_typeofArgName`（FQ + 递归带尖括号）而非 `Z42Type.Name()`（短名 + `", "` 分隔），使 `obj.GetType().GetGenericArguments()` 的嵌套实参解析为**真句柄 + 递归**、与 `typeof` 完全一致（此前短名不解析致 `inner.IsGenericType==false`、`", "` 空格致 arg 名带前导空格）。复用已合入的 runtime 括号解析，**无 runtime 改动、无格式 bump**。泛型实参内嵌数组的 `typeof` 语法（`typeof(Box<int[]>)`）同 jagged 数组延后。

### ~~reflection-future-instance-generic-args~~ — 已落地（2026-06-16）
- **状态**：`new Box<int>()` 实例的 `obj.GetType().GetGenericArguments()` 返 `[int]` 已落地 2026-06-16（add-reflection-instance-generic-args）。`__obj_get_type` 对泛型实例（`ScriptObject.type_args` 非空）调 `make_constructed_type(td.name, type_args)`——与 `typeof(Box<int>)` 产出**同一构造型 Type**（__typeArgs 槽）。`obj.GetType()` 与 `typeof` 对构造泛型的反射结果（GetGenericArguments / IsGenericTypeDefinition）一致。纯运行期，**无格式 bump**。
- **剩余**：嵌套泛型实例（`new Box<Map<K,V>>()`）arg 仍扁平名串 → `reflection-future-nested-generic-args`。

### ~~reflection-future-type-category-flags~~ — 部分落地（2026-06-16，IsClass + IsInterface）

- **状态**：`Type.IsClass` / `Type.IsInterface` 已落地 2026-06-16（add-reflection-interface-class-predicates，zbc 1.19 / zpkg 0.21）。**interface 现在 emit 最小 TYPE 条目**（此前完全不产 → `typeof(IFoo)` 是 name-only），`class_flags` 扩 bit4 = interface。`IsInterface` 读 bit4；`IsClass` = 有句柄且 `!struct && !interface`（记录是 class）。接口隐式 abstract（`typeof(IFoo).IsAbstract == true`）。**根因修**：`Z42TypeName` 此前不处理 `Z42InterfaceType`（落 `ToString` 未限定名）→ 加 `QualifyClassName` 分支，否则 typeof(IFoo) 漏句柄。dotnet 1565/1565 + `interface_class_predicates.z42` e2e（interp+jit）+ cargo 809/0。
- **剩余**：`IsEnum`（bit5 预留）——见下方 `reflection-future-isenum`。~~接口成员枚举（`typeof(IFoo).GetMethods()`）~~ —— 已落地 2026-07-20（add-interface-member-reflection，纯运行期：surface 现有 zbc 1.28 接口方法块——`fix-crosspkg-interface-impl` 已 emit，reader 原丢弃改存入 `TypeDesc.iface_methods` → GetMethods 从签名直接构建 MethodInfo（IsAbstract/IsVirtual=true，params 带类型无源名）；只返直接声明方法，无 compiler 改动、无格式 bump）。~~接口继承接口方法（transitive）~~ —— 已落地 2026-07-24（add-reflection-transitive-interface-methods，纯运行期：`typeof(IBar).GetMethods()` 对接口 BFS 展开其基接口闭包（复用 `GetInterfaces` 的 `interfaces()` 遍历），并入各基接口的 `iface_methods`（按**声明接口**限定名 dedup）——`interface IBar : IFoo` → `typeof(IBar).GetMethods()` 含 IFoo 的方法，镜像 C#。仅对接口生效（类的具体实现已在 vtable，不重复灌抽象签名）；无 compiler 改动、无格式 bump）。数组 `IsClass==true`（C# 语义，z42 数组 name-only）延后。

### reflection-future-isenum
- **来源**：add-reflection-interface-class-predicates Out of Scope
- **触发原因**：`Type.IsEnum` 需 enum **作为类型实体**——z42 enum 当前底层只是 int 常量字典，无类型实体、不产 TYPE 条目。
- **前置依赖**：enum 类型实体设计（features.md / 独立 design doc）+ enum TYPE 条目（带 bit5 类别位 + underlying type + 成员名/值）。
- **触发条件**：需要 `IsEnum` / `GetEnumNames` / `GetEnumValues` / `Enum.Parse` 时（独立 change，先做 enum 类型系统设计）。

### reflection-future-method-invoke（0.5.x L3-R）
- **来源**：roadmap 0.3.x C 主线划界
- **触发原因**：`Method.Invoke` / `Activator.CreateInstance<T>()` / `Type.MakeGenericType` 强依赖 generic instantiation。
- **触发条件**：0.5.x 泛型 instantiation 落地后，于独立 `z42.reflection` 包提供。

### ~~reflection-future-attributes（C3）~~ — 已落地（class + method + field + parameter）
- **状态**：用户自定义 attribute + 反射全落地。class-level（C3a，2026-06-09）+ method-level（C3b，2026-06-09）+ field-level（add-field-attribute-reflection，zbc 1.14，2026-06-10）+ **parameter-level（add-parameter-attribute-reflection，zbc 1.15，2026-06-10）**。`[Foo(args)]` 标注 class/method/field/parameter → `Type` / `MethodInfo` / `FieldInfo` / `ParameterInfo` 的 `GetCustomAttributes()` / `GetAttribute(Type)` 返活实例（缓存）。parameter attr 持久化进 SIGS section 每参数 attr-ref 块，运行期 `FunctionCold.param_attributes`，`__param_custom_attributes(qualified, position)` 按源参数位置取（wire 索引 = position + this 偏移）。设计 + 实现原理见 [attributes.md](attributes.md)。
- **剩余**：AttributeUsage（目标校验）、泛型 attribute、专用诊断 —— 见 attributes.md Deferred。所有声明目标（class/method/field/parameter）已覆盖。

### attr-factory-return-type-resolution — 合成 attribute factory 返回类型解析为 PrimType（compiler bug）

- **来源**：fix-reflection-test-compile（2026-06-10，reflection.z42 编译失败暴露）；fix-qualified-base-upcast 调查（2026-06-10）证伪了"FQN-upcast"初判、定位到真根因。
- **现象**：`class FieldTag : Std.Attribute`（**限定基名**）时，`AttributeFactorySynthesizer` 合成的 `public Attribute __f() { return new FieldTag(...); }` 报 `return type mismatch: expected Attribute, got FieldTag`（[TypeChecker.Stmts.cs:421](../../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Stmts.cs)）。
- **真根因（调查更正）**：**不是** `IsSubclassOf` 的 FQN 比较问题（`RequireAssignable` 的 `IsSubclassOf` 分支根本没被命中）。插桩显示 `target = Z42PrimType:Attribute` —— 合成 factory 的返回类型 `NamedType("Attribute")` 在**合成顶层函数的解析上下文**里**未解析到 `Std.Attribute` 类，回落成 sentinel `Z42PrimType("Attribute")`**（改成限定 `NamedType("Std.Attribute")` 也回落成 `Z42PrimType:Std.Attribute`）。同一文件里用户手写 `Std.Attribute[] attrs = ...`（reflection.z42:245）作类型注解却能解析 → 差异在**合成函数体的类型解析上下文**（疑似拿不到 file 的 using / imported-class 解析）。属上游 name-resolution 缺陷,非 assignability。
- **诡异点（待解释）**：stdlib-test 编译路径下 unqualified `: Attribute` **能**编译过（reflection.z42 现状即此),但等价单文件 repro（`using Std` + `class Tag : Attribute`）**两种基名都**回落 PrimType。说明失败与"编译上下文(workspace/imported vs 单文件)"强相关。
- **试过无效**：① `IsSubclassOf`/`_ancestors` 短名归一(走不到该分支);② 合成 factory 返回类型改限定 `Std.Attribute`(仍回落 PrimType)。均已回退。
- **当前 workaround**：attribute 类基名写 unqualified `: Attribute`（reflection.z42 即此,绿）。
- **前置依赖 / 触发条件**：根因在 name-resolution(合成函数如何解析 imported 类型),需较深 compiler 调查;与进行中 `port-z42c-*` 自举移植可能冲突 → 独立 change，自举收口后再做。

### fold-nonliteral-param-defaults — 非字面量参数默认值的值

- **来源**：add-param-metadata（unify P1-d，2026-07-10）Out of Scope。
- **触发原因**：`ParameterInfo.DefaultValue` 只折**字面量**默认值（Int/Float/Bool/String/Char/null
  直取）；常量表达式（`1+2`）、enum 成员、命名常量默认值需常量折叠器，本砖未建。
- **前置依赖**：add-param-metadata（default_kind 编码已落，扩展只需 IrGen 折叠更多 Expr 形态）。
- **触发条件**：反射需读非字面量默认值的值（named-args 求值 / 文档生成）时。
- **当前 workaround**：非字面量默认值 `DefaultValue == null`（kind=0），`IsOptional` 仍 true。


### reflection-future-typeof-delegate — typeof(委托名) 直达句柄

- **来源**：add-delegate-metadata（unify P1-e ②，2026-07-11）Out of Scope。
- **触发原因**：delegate 名在 `ResolveTypeP` 解析为 Z42FuncType（delegate 赋值/类型检查依赖），
  typeof 发射路径需对 delegate 名特判（Z42FuncType → FQ delegate 名 → Typeof opcode）。
- **前置依赖**：本砖（TYPE 条目已存在，运行期可解析）。
- **触发条件**：用户需要 `typeof(MyDelegate)` 字面形式时。
- **当前 workaround**：`Type.GetType("<fq delegate>")`。
