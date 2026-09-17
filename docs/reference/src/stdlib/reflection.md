# 反射 —— 运行时检视、构造与调用类型

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`；命名空间 `Std`（`Type` / `Enum` / `Attribute` /
> `TypeVisibility` / `Array`）与 `Std.Reflection`（`MemberInfo` / `MethodBase` /
> `MethodInfo` / `ConstructorInfo` / `FieldInfo` / `PropertyInfo` / `ParameterInfo` /
> `Activator` / `Assembly`）

反射让程序在运行时拿到类型的元数据，并据此**读写字段与属性、调用方法、构造实例**。
API 形态对标 C# `System.Type` / `System.Reflection.*`：`Std.Type` 是唯一的类型句柄
（没有 C# 的 `Type` / `TypeInfo` 拆分），成员描述符都在 `Std.Reflection`。

三个入口：

| 入口 | 拿到什么 |
|---|---|
| `typeof(T)` | 编译期已知的类型 → `Std.Type`（`Std` 在 prelude，无需 `using`）|
| `obj.GetType()` | 运行期实例的类型（`Std.Object` 的成员，每个引用类型都有）|
| `Type.GetType(string fqn)` | 按名字查，未命中返 `null` |

`typeof(int)` 与 `(5).GetType()` 是同一个类型身份（都是 `Std.Int32`），`typeof(Box<int>)`
与 `new Box<int>().GetType()` 也是。

```z42
using Std.IO;
using Std.Reflection;

class Point {
    public int X;
    public int Y;
    public int Dist2() { return this.X * this.X + this.Y * this.Y; }
}

void Main() {
    Type t = typeof(Point);
    Console.WriteLine(t.FullName);                 // Point
    foreach (FieldInfo f in t.GetFields()) {
        Console.WriteLine(f.Name + " : " + f.FieldType.Name);   // X : Int32 / Y : Int32
    }
    Point p = new Point();
    p.X = 3;
    Console.WriteLine(p.GetType().BaseType.Name);   // Object（链式属性访问正常）
}
```

> `Std.Type`、`FieldInfo` 等类上以 `__` 开头的 public 成员（`__name` / `__qualified` /
> `__typeArgs` …）是 VM 写入的存储槽，不是 API；它们会出现在对这些类自身的反射结果里，
> 但不应直接读写。

## `Std.Type`

类型句柄。用户不能 `new Type()`——实例只由 `typeof` / `GetType()` / 反射 builtin 产出。
`sealed`，继承 `Std.Reflection.MemberInfo`（故 `typeof(C) is MemberInfo` 为真，`Name`
来自基类）。

### 身份与名字

```z42
public string Name;                              // 继承自 MemberInfo（字段）
public extern string FullName { get; }
public extern Type BaseType { get; }
public static extern Type GetType(string fqn);
public extern Std.Reflection.Assembly Assembly { get; }
```

| 成员 | 说明 |
|---|---|
| `Name` | 简单名（`Point` / `Int32` / `Box` / `Inner`）|
| `FullName` | 限定名，含命名空间；构造泛型带实参；嵌套类型用 `+` |
| `BaseType` | 基类；无显式基类的类为 `Std.Object`；`Std.Object` 自身与数组合成 Type 为 `null` |
| `GetType(fqn)` | 限定名查表；未知返 `null`。短名（`Int32`）有兜底解析 |
| `Assembly` | 定义该类型的 zpkg 投影 |

**名字口径**（写断言前先看这张表，与 C# 源关键字拼写**不同**）：

| 类型 | `Name` | `FullName` |
|---|---|---|
| `int` | `Int32` | `Std.Int32` |
| `string` | `String` | `Std.String` |
| 用户类 `ns.Point` | `Point` | `ns.Point` |
| `int[]` | `Int32[]` | `Std.Int32[]` |
| `int[][]` | `Int32[][]` | — |
| `Box<int>` | `Box` | `Box<Std.Int32>`（嵌套递归展开，逗号无空格）|
| 嵌套 `Outer.Inner` | `Inner` | `Outer+Inner` |

反射返回的名字一律是 BCL wrapper 拼写（`Int32` 而非 `int`）；`Type.GetType` 两种拼写
都接受，嵌套类型只认 `+` 形式（`Type.GetType("Outer.Inner")` 返 `null`）。

### 类别谓词

```z42
public bool IsArray;                                   // 字段，非属性
public extern bool IsClass { get; }
public extern bool IsValueType { get; }
public extern bool IsInterface { get; }
public extern bool IsEnum { get; }
public extern bool IsRecord { get; }
public extern bool IsDelegate { get; }
public extern bool IsPrimitive { get; }
public extern bool IsAbstract { get; }
public extern bool IsSealed { get; }
public extern bool IsGenericType { get; }
public extern bool IsGenericTypeDefinition { get; }
public extern bool IsNested { get; }
public extern bool IsCollectible { get; }
```

| 谓词 | 为 true 的情形 |
|---|---|
| `IsClass` | 引用类型（含 `record`）；struct / interface / enum / 基元 / 数组为 false |
| `IsValueType` | `struct` 与基元包装类型（`typeof(int).IsValueType == true`）|
| `IsRecord` | 声明带 `record` |
| `IsDelegate` | `delegate` 声明产生的类型 |
| `IsPrimitive` | `int` / `long` / `bool` / `char` / `double` … ——**`string` 不算基元**（对齐 C#）|
| `IsAbstract` / `IsSealed` | 声明修饰符，**不随继承传播**；接口隐式 `IsAbstract == true` |
| `IsGenericType` | 该类型有类型参数（定义或已构造均为 true）|
| `IsGenericTypeDefinition` | 开放定义 `Box<>`；`typeof(Box<int>)` 为 false |
| `IsArray` | 数组类型（`typeof(int[])`、数组字段/形参类型、`arr.GetType()`）|
| `IsCollectible` | 类型载入了可回收的 `AssemblyLoadContext`；常驻 root 上下文的类型为 false |

### 可见性

```z42
public extern TypeVisibility Visibility { get; }
public bool IsPublic         { get; }
public bool IsNotPublic      { get; }
public bool IsNestedPublic   { get; }
public bool IsNestedPrivate  { get; }
public bool IsNestedFamily   { get; }
public bool IsNestedAssembly { get; }
```

`Std.TypeVisibility` 是四档 enum：`Public`（0）/ `Private`（1）/ `Protected`（2）/
`Internal`（3）。顶层类型只会是 `Public` 或 `Internal`（顶层 `private` / `protected`
在编译期被拒），嵌套类型四档都可能。六个 bool 是 `Visibility × IsNested` 的组合，
与 C# 的同名属性一一对应。

> 不写修饰符的顶层类型是 `Internal`，所以 `typeof(MyClass).IsPublic` 对未加 `public`
> 的类是 `false`。无句柄的合成 Type（数组）报 `Public` + `IsNested == false`。

### 成员枚举

```z42
public extern Std.Reflection.FieldInfo[]       GetFields();
public extern Std.Reflection.MethodInfo[]      GetMethods();
public extern Std.Reflection.ConstructorInfo[] GetConstructors();
public extern Std.Reflection.PropertyInfo[]    GetProperties();
public extern Std.Reflection.MemberInfo[]      GetMembers();
public extern Type[] GetNestedTypes();
public extern Type   GetDeclaringType();
public extern Type   GetElementType();
```

| 方法 | 返回 |
|---|---|
| `GetFields()` | 实例字段（含继承，base 在前）+ 本类与祖先类的静态字段。**含 private / protected 字段**；auto-property 的合成后备字段 `__prop_<Name>` 被隐藏 |
| `GetMethods()` | 方法（含继承与虚方法）。属性访问器 `get_X` / `set_X` 也在其中（与 C# 一致）；**不含构造器**。对接口返回其声明方法 + 基接口方法的传递闭包 |
| `GetConstructors()` | 本类型声明的构造器。基元 / 数组 / 接口返回空 |
| `GetProperties()` | 由 `get_<X>` / `set_<X>` 访问器约定派生的属性视图（含继承、含静态属性）。getter 使其可读、setter 使其可写、同名合并为一条 |
| `GetMembers()` | 字段 + 方法 + 属性 + 直接嵌套类型（**不含构造器**）|
| `GetNestedTypes()` | 直接嵌套于本类型的类型（不含更深层、不含继承），有序 |
| `GetDeclaringType()` | 嵌套类型的直接外层；非嵌套返 `null` |
| `GetElementType()` | 数组的元素类型；非数组返 `null` |

基元类型有真句柄，成员照样枚举得到（`typeof(int).GetFields()` 给出 `MaxValue` /
`MinValue` 两个静态字段，`typeof(string).GetMethods()` 给出 `Std.String` 的全部方法）。
只有数组是无句柄的合成 Type：`Name` / `FullName` / `IsArray` / `GetElementType()` 有值，
其余成员查询一律返回**空数组**或 `null`，不报错。

### 泛型

```z42
public extern Type[] GetGenericArguments();
public extern Type   GetGenericTypeDefinition();
public extern Type   MakeGenericType(params Type[] typeArgs);
```

```z42
Type bi = typeof(Box<int>);
bi.IsGenericTypeDefinition;                // false（已构造）
bi.GetGenericArguments()[0].Name;          // "Int32"
Type def = bi.GetGenericTypeDefinition();  // Box<>
def.IsGenericTypeDefinition;               // true；GetGenericArguments() 为空
def.MakeGenericType(typeof(string));       // Box<Std.String>
```

- `GetGenericTypeDefinition()` 对非泛型类型抛 `Std.Exception`
  （`GetGenericTypeDefinition: type is not a generic type`）。
- `MakeGenericType` 校验实参个数与 `where` 约束，违反抛 `Std.Exception`
  （`MakeGenericType: 'Box' expects 1 type argument(s), got 2`）。
- 嵌套泛型实参递归还原：`typeof(Box<Pair<int,string>>)` 的
  `GetGenericArguments()[0].GetGenericArguments()` 给出 `[Int32, String]`。

### 接口与赋值兼容

```z42
public extern Type[] GetInterfaces();
public        Type   GetInterface(string name);
public extern bool   IsAssignableFrom(Type c);
```

`GetInterfaces()` 给出**本类直接声明的 + 沿基类链继承的 + 接口继承接口的传递闭包**
（按限定名 dedup），每项都是真接口句柄（`IsInterface` / `FullName` 可用）。无接口返回
空数组，不是 `null`。

`GetInterface(name)` 按**简单名精确匹配**（大小写敏感，无 `ignoreCase` 重载），未命中
返 `null`。

`IsAssignableFrom(c)` 是反射版的 `is`：`c` 是本类型、派生自本类型、或实现本接口时为
真；`null` 恒为 false；`typeof(object)` 对任意非 null 类型为真。

### enum 元数据

```z42
public extern bool IsEnum { get; }
public extern Type GetEnumUnderlyingType();
```

`GetEnumUnderlyingType()` 恒返 `typeof(long)`——z42 一律以 i64 背书 enum，声明的
`: byte` 当前被忽略。非 enum 抛 `Std.Exception`。成员名/值经 `Std.Enum`（见下）。

### 类型上的 attribute

```z42
public Attribute[] GetCustomAttributes();
public Attribute   GetAttribute(Type attributeType);
```

返回**活实例**（编译器为每处应用合成一个工厂，VM 执行它）。首次调用物化并缓存，
之后每次返回**同一批实例**。`GetAttribute` 按实例运行期类型的 `FullName` 匹配，未命中
返 `null`。attribute 的声明规则见[特性](../language/attributes.md)。

## `Std.Reflection.MemberInfo` / `MethodBase`

```z42
public class MemberInfo {
    public string Name;                  // 字段，VM 写入
}

public class MethodBase : MemberInfo {
    public bool IsStatic;
    public ParameterInfo[] GetParameters();   // 声明序，不含隐式 this
}
```

`MemberInfo` 是 `Type` / `FieldInfo` / `MethodInfo` / `ConstructorInfo` / `PropertyInfo`
的共同基类。`ParameterInfo` **不**继承它。

## `Std.Reflection.MethodInfo`

```z42
public class MethodInfo : MethodBase {
    public Std.Type ReturnType;
    public bool IsVirtual;
    public bool IsAbstract;
    public bool IsSealed;
    public bool IsPublic;
    public bool IsPrivate;

    public extern object Invoke(object obj, object[] args);

    public bool IsGenericMethod;
    public bool IsGenericMethodDefinition;
    public extern Std.Type[]  GetGenericArguments();
    public extern MethodInfo  MakeGenericMethod(params Std.Type[] typeArgs);

    public Std.Attribute[] GetCustomAttributes();
    public Std.Attribute   GetAttribute(Std.Type attributeType);
}
```

| 成员 | 说明 |
|---|---|
| `ReturnType` | 返回类型；`void` 方法的 `ReturnType.Name` 与 `FullName` 都是 `"void"` |
| `IsVirtual` | `virtual` / `override` / `abstract` 任一（三者都走虚派发）|
| `IsAbstract` | `abstract`（同时 `IsVirtual == true`）|
| `IsSealed` | `sealed override`（同时 `IsVirtual == true`）|
| `IsPublic` / `IsPrivate` | 声明修饰符。`protected` 方法**两者皆 false**（镜像 C# `IsFamily`）|

### `Invoke`

```z42
object r = m.Invoke(receiver, args);
```

- **静态方法**：`obj` 传 `null`（被忽略）；**实例方法**：`obj` 是接收者。
- `args` 是 `object[]`，按声明序映射形参；基元实参自动装箱，返回值需调用方 `(T)` 拆箱。
- `void` 方法返回 `null`。
- 被调方法内的 `throw` 以**原始异常类型**传播，调用方可 `try` / `catch` 精确匹配。
- 实参个数不符抛 `Std.Exception`：
  `MethodInfo.Invoke: 'Boom.Blow' expects 1 argument(s) (incl. receiver), got 2`。

### 泛型方法

```z42
MethodInfo def = /* 从 GetMethods() 取 */;
def.IsGenericMethod;              // true
def.IsGenericMethodDefinition;    // true —— 未绑定
MethodInfo bound = def.MakeGenericMethod(typeof(int));
bound.IsGenericMethodDefinition;  // false
object r = bound.Invoke(null, new object[0]);
```

在**构造后**的 `MethodInfo` 上 `Invoke`，方法体内的 `typeof(T)` / `new T()` /
`default(T)` 与直接写 `Foo<int>()` 完全一致。`GetGenericArguments()` 在定义上给出类型
参数占位，在构造后给出绑定的实参。对非泛型定义调 `MakeGenericMethod`、或实参个数不符，
抛 `Std.Exception`。

## `Std.Reflection.ConstructorInfo`

```z42
public class ConstructorInfo : MethodBase {
    public extern object Invoke(object[] args);
}
```

`Type.GetConstructors()` 枚举。`Invoke` 分配一个**新实例**、用 `args` 跑构造体、返回构造
出的对象——与 `MethodInfo.Invoke`（在已有接收者上调方法）不同。构造器永不静态，
继承来的 `IsStatic` 恒 false。`Name` 是类型的简单名。实参个数不符抛 `Std.Exception`；
构造体内的 `throw` 以原类型传播。

按参数类型挑重载没有现成方法——枚举后自己看 `GetParameters()`。

## `Std.Reflection.FieldInfo`

```z42
public class FieldInfo : MemberInfo {
    public Std.Type FieldType;
    public bool IsStatic;
    public bool IsPublic;
    public bool IsPrivate;

    public extern object GetValue(object obj);
    public extern void   SetValue(object obj, object value);

    public Std.Attribute[] GetCustomAttributes();
    public Std.Attribute   GetAttribute(Std.Type attributeType);
}
```

`GetValue` / `SetValue` 直接读写实例字段的槽（字段就是槽，不经访问器），是反射式
(反)序列化落在公开字段上的通路。`protected` 字段 `IsPublic == IsPrivate == false`。

## `Std.Reflection.PropertyInfo`

```z42
public class PropertyInfo : MemberInfo {
    public Std.Type PropertyType;
    public bool CanRead;
    public bool CanWrite;

    public extern object GetValue(object obj);
    public extern void   SetValue(object obj, object value);

    public Std.Attribute[] GetCustomAttributes();
    public Std.Attribute   GetAttribute(Std.Type attributeType);
}
```

- `PropertyType` 取 getter 的返回类型；只写属性取 setter 的值形参类型。
- `GetValue` / `SetValue` 反射调用 `get_<X>` / `set_<X>`。只读属性 `SetValue`、只写属性
  `GetValue` 抛 `Std.Exception`；访问器内的 `throw` 以原类型传播。
- 按**声明类**的访问器调用，不做虚 override 派发。
- auto-property 的 attribute 挂在合成后备字段上并由此解析；纯计算属性（无后备字段）
  没有 attribute。

## `Std.Reflection.ParameterInfo`

```z42
public class ParameterInfo {
    public string   Name;
    public Std.Type ParameterType;
    public int      Position;
    public bool     IsOptional;
    public bool     IsParams;
    public object   DefaultValue;

    public Std.Attribute[] GetCustomAttributes();
    public Std.Attribute   GetAttribute(Std.Type attributeType);
}
```

| 成员 | 说明 |
|---|---|
| `Name` | 源参数名。剥了 debug 符号的 release 包也有 |
| `Position` | 0-based 逻辑位置，**不含隐式 `this`** |
| `IsOptional` | 该参数有默认值。可选参数恒尾随，`params` 参数也报 true |
| `IsParams` | 该参数是 `params` 变长参数 |
| `DefaultValue` | 折出的常量默认值，没有则 `null` |

`DefaultValue` 折**字面量与常量表达式**（`1 + 2` → `3`、`-5`、`!false`、`1 << 4`）。
折不出来的（enum 成员 `Color.Green`、命名常量 `K`、字符串拼接）给 `null`，但
`IsOptional` 仍为 `true`——**`DefaultValue == null` 不代表没有默认值**，要判有无用
`IsOptional`。

## `Std.Reflection.Activator`

```z42
public static class Activator {
    public static extern object CreateInstance(Std.Type t);
    public static T CreateInstance<T>();
}
```

按无参构造器反射 `new`（类型没有显式构造器时就是默认字段分配）。构造器里的 `throw`
以原类型传播。带参构造走 `ConstructorInfo.Invoke`。

## `Std.Reflection.Assembly`

```z42
public sealed class Assembly {
    public extern string Name { get; }
    public extern bool   IsCollectible { get; }
    public extern Std.Runtime.AssemblyLoadContext LoadContext { get; }
    public extern Std.Type[] GetTypes();     // 按 FQ 名有序
}
```

一个 zpkg 的运行时投影。不可用户构造——由 `Type.Assembly` /
`AssemblyLoadContext.Load` / `AssemblyLoadContext.GetAssemblies()` 产出。常驻 root
上下文的类型报告合成的 root assembly（`Name` 为 `"root"`，`GetTypes()` 返回空数组）。

## `Std.Enum`

```z42
public static class Enum {
    public static extern string[] GetNames(Type enumType);      // 声明序
    public static extern long[]   GetValues(Type enumType);     // 声明序，底层 i64
    public static extern string   GetName(Type enumType, long value);
    public static extern long     Parse(Type enumType, string name);
    public static extern bool     IsDefined(Type enumType, long value);
}
```

- 值一律以 `long` 进出（z42 enum 底层是 i64）。
- `GetName` 未命中返回 **空串**（不是 `null`）。
- `Parse` 未命中抛 `Std.Exception`；**大小写敏感**，没有 `ignoreCase` 变体。
- 非 enum 或无句柄的 Type：`GetNames` / `GetValues` 返回空数组，`IsDefined` 返回 false。

enum 值本身也携带类型：`Color.Green.ToString()` 给 `"Green"`，
`Color.Green.GetType().IsEnum` 为真。enum 的声明语法见 [enum](../language/enums.md)。

## `Std.Attribute`

```z42
public class Attribute { }
```

所有用户 attribute 的基类——空基类，行为全在反射侧的 `GetCustomAttributes()` /
`GetAttribute()`（`Type` / `MethodInfo` / `FieldInfo` / `PropertyInfo` / `ParameterInfo`
五处各有一份，形状相同）。声明与应用规则（`Attribute` 后缀强制、`[Use]` ↔ `UseAttribute`
映射）见[特性](../language/attributes.md)。

## 数组的反射式建/读写

`Std.Array` 是所有 `T[]` 的运行期基类。以下几个成员用于**编译期不知元素类型**时操作
数组（其余静态算法属于数组 API，不在反射范畴）：

```z42
public static extern Array CreateInstance(Type elementType, int length);
public extern Object GetValue(int index);
public extern void   SetValue(Object value, int index);
public int Length;
```

```z42
Array a = Array.CreateInstance(typeof(int), 3);
a.SetValue(10, 0);                 // 注意实参序是 (value, index)，与 C# 一致
object v = a.GetValue(0);          // 基元元素装箱返回
```

## 不支持

- **没有按名取单个成员的方法**：`GetField(name)` / `GetMethod(name)` /
  `GetProperty(name)` / `GetConstructor(Type[])` 都不存在，枚举后自己筛。
  `GetInterface(name)` 是唯一的按名查找便利方法。
- **`Activator.CreateInstance(Type, args)` 不存在**——带参构造用
  `ConstructorInfo.Invoke(args)`。
- **索引器不出现在 `GetProperties()`**：`this[int]` 降解为 `get_Item(int)`，逻辑参数
  个数不符 getter 约定，只在 `GetMethods()` 里作为普通方法出现。
- **`PropertyInfo.GetValue` / `SetValue` 不做虚 override 派发**——调声明类的访问器。
- **没有写入型的类型元数据**：不能新增/修改成员，`Type` 不可用户构造。
- **enum**：没有 `Enum.TryParse`，也不支持 `[Flags]` 的组合名解析/渲染。
- **多维数组 `T[,]` 没有类型语法**，因此无从反射（交错数组 `T[][]` 可以）。
- **泛型 delegate 的 `typeof`**（`typeof(MyGenericDel<int>)`）与匿名函数类型
  `typeof((int) -> void)` 不解析到 delegate 句柄。
- **反射只看得见已加载的包**：某个 zpkg 未被加载时，它为别的类型加的
  `impl Trait for T` 不会出现在 `GetInterfaces()` 里（那种情形下该方法本也调不到）。

## 关联页面

- [特性（Attributes）](../language/attributes.md) —— attribute 的声明与应用
- [enum（枚举）](../language/enums.md) —— enum 声明语法
- [嵌套类型](../language/nested-types.md) —— `Outer.Inner` 的语法与 `+` 名约定
- [泛型方法](../language/generic-methods.md) —— 方法级类型参数
- [访问权限控制](../language/access-control.md) —— `Visibility` 报告的四档修饰符
