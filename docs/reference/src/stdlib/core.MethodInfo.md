<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MethodInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L8)

反射方法，对标 C# `System.Reflection.MethodInfo`，由 `Type.GetMethods()` 填充。

泛型方法从泛型定义（`Foo<T>()`）反射时是「定义」（IsGenericMethodDefinition == true）；
`MakeGenericMethod` 绑定类型实参后返回「构造后」的 MethodInfo。构造后 Invoke 时方法体内的
`typeof(T)` / `new T()` / `default(T)` 与直接写 `Foo<Arg>()` 完全一致。

```z42
public class MethodInfo : MethodBase {
    Std.Type ReturnType
    bool IsVirtual
    bool IsAbstract
    bool IsSealed
    bool IsPublic
    bool IsPrivate
    static bool op_Equality(MethodInfo a, MethodInfo b)
    static bool op_Inequality(MethodInfo a, MethodInfo b)
    override bool Equals(object other)
    override int GetHashCode()
    Std.Attribute[] __attrCache
    extern Std.Attribute[] __customAttributes(string qualified)
    Std.Attribute[] GetCustomAttributes()
    Std.Attribute? GetAttribute(Std.Type attributeType)
    extern object Invoke(object obj, object[] args)
    bool IsGenericMethod
    bool IsGenericMethodDefinition
    string[] __typeParamNames
    Std.Type[] __typeArgs
    extern Std.Type[] GetGenericArguments()
    extern MethodInfo MakeGenericMethod(params Std.Type[] typeArgs)
}
```

成员：[`ReturnType`](#Std.Reflection.MethodInfo.ReturnType) · [`IsVirtual`](#Std.Reflection.MethodInfo.IsVirtual) · [`IsAbstract`](#Std.Reflection.MethodInfo.IsAbstract) · [`IsSealed`](#Std.Reflection.MethodInfo.IsSealed) · [`IsPublic`](#Std.Reflection.MethodInfo.IsPublic) · [`IsPrivate`](#Std.Reflection.MethodInfo.IsPrivate) · [`op_Equality`](#Std.Reflection.MethodInfo.op_Equality(MethodInfo,MethodInfo)) · [`op_Inequality`](#Std.Reflection.MethodInfo.op_Inequality(MethodInfo,MethodInfo)) · [`Equals`](#Std.Reflection.MethodInfo.Equals(object)) · [`GetHashCode`](#Std.Reflection.MethodInfo.GetHashCode()) · [`__attrCache`](#Std.Reflection.MethodInfo.__attrCache) · [`__customAttributes`](#Std.Reflection.MethodInfo.__customAttributes(string)) · [`GetCustomAttributes`](#Std.Reflection.MethodInfo.GetCustomAttributes()) · [`GetAttribute`](#Std.Reflection.MethodInfo.GetAttribute(Std.Type)) · [`Invoke`](#Std.Reflection.MethodInfo.Invoke(object,object[])) · [`IsGenericMethod`](#Std.Reflection.MethodInfo.IsGenericMethod) · [`IsGenericMethodDefinition`](#Std.Reflection.MethodInfo.IsGenericMethodDefinition) · [`__typeParamNames`](#Std.Reflection.MethodInfo.__typeParamNames) · [`__typeArgs`](#Std.Reflection.MethodInfo.__typeArgs) · [`GetGenericArguments`](#Std.Reflection.MethodInfo.GetGenericArguments()) · [`MakeGenericMethod`](#Std.Reflection.MethodInfo.MakeGenericMethod(Std.Type[]))

<a id="Std.Reflection.MethodInfo.ReturnType"></a>
### Std.Type ReturnType

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L10)

返回类型，如 `typeof(int)` / `typeof(void)`。VM 写入。

<a id="Std.Reflection.MethodInfo.IsVirtual"></a>
### bool IsVirtual

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L15)

是否为 virtual / override / abstract 方法（三者都走虚派发）。

<a id="Std.Reflection.MethodInfo.IsAbstract"></a>
### bool IsAbstract

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L18)

是否为 abstract 方法（同时 IsVirtual == true）。

<a id="Std.Reflection.MethodInfo.IsSealed"></a>
### bool IsSealed

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L21)

是否为 `sealed override`（或简写 `sealed`）方法——不允许再 override（同时 IsVirtual == true）。

<a id="Std.Reflection.MethodInfo.IsPublic"></a>
### bool IsPublic

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L24)

声明为 public。

<a id="Std.Reflection.MethodInfo.IsPrivate"></a>
### bool IsPrivate

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L26)

声明为 private。`protected` 方法两者皆 false（镜像 C#）。

<a id="Std.Reflection.MethodInfo.op_Equality(MethodInfo,MethodInfo)"></a>
### static bool op_Equality(MethodInfo a, MethodInfo b)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L34)

按 `__qualified`（全限定方法名）判断两个 `MethodInfo` 是否指向同一方法。

<a id="Std.Reflection.MethodInfo.op_Inequality(MethodInfo,MethodInfo)"></a>
### static bool op_Inequality(MethodInfo a, MethodInfo b)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L42)

不等运算符，等价于 `!(a == b)`。

<a id="Std.Reflection.MethodInfo.Equals(object)"></a>
### override bool Equals(object other)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L50)

值相等：`other` 为 `MethodInfo` 且 `__qualified` 相同才为真（与 `==` 一致）。

<a id="Std.Reflection.MethodInfo.GetHashCode()"></a>
### override int GetHashCode()

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L57)

哈希码取自 `__qualified`，与 Equals 的判据一致。

<a id="Std.Reflection.MethodInfo.__attrCache"></a>
### Std.Attribute[] __attrCache

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L62)

缓存的 attribute 实例（缓存理由见 Type.GetCustomAttributes）。

<a id="Std.Reflection.MethodInfo.__customAttributes(string)"></a>
### extern Std.Attribute[] __customAttributes(string qualified)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L66)

native 门面：取 `qualified` 全限定名对应方法上的 attribute 实例数组。

<a id="Std.Reflection.MethodInfo.GetCustomAttributes()"></a>
### Std.Attribute[] GetCustomAttributes()

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L69)

本方法上应用的 attribute，以活实例返回；首次调用后缓存。

<a id="Std.Reflection.MethodInfo.GetAttribute(Std.Type)"></a>
### Std.Attribute? GetAttribute(Std.Type attributeType)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L78)

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

<a id="Std.Reflection.MethodInfo.Invoke(object,object[])"></a>
### extern object Invoke(object obj, object[] args)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L96)

调用本方法：实例方法 `obj` 是接收者（静态方法传 `null` 即可）；`args` 按声明序映射形参。

返回方法返回值（`void` → null）；被调方法内的 `throw` 以原类型传播。仅限非泛型方法。

<a id="Std.Reflection.MethodInfo.IsGenericMethod"></a>
### bool IsGenericMethod

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L101)

方法是否声明了类型参数（定义或已构造均为 true）。

<a id="Std.Reflection.MethodInfo.IsGenericMethodDefinition"></a>
### bool IsGenericMethodDefinition

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L104)

是否为未绑定的泛型方法定义；经 `MakeGenericMethod` 绑定实参后为 false。

<a id="Std.Reflection.MethodInfo.__typeParamNames"></a>
### string[] __typeParamNames

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L109)

声明的类型参数名（如 `["T","U"]`），VM 写入。

定义上 GetGenericArguments() 据此生成占位 Type。

<a id="Std.Reflection.MethodInfo.__typeArgs"></a>
### Std.Type[] __typeArgs

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L114)

构造后 MethodInfo 上由 MakeGenericMethod 绑定的类型实参；定义上为 null。

Invoke 时传入被调帧。

<a id="Std.Reflection.MethodInfo.GetGenericArguments()"></a>
### extern Std.Type[] GetGenericArguments()

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L118)

方法的类型参数（定义上）或绑定的类型实参（构造后）。

<a id="Std.Reflection.MethodInfo.MakeGenericMethod(Std.Type[])"></a>
### extern MethodInfo MakeGenericMethod(params Std.Type[] typeArgs)

[`src/libraries/z42.core/src/Reflection/MethodInfo.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodInfo.z42#L123)

给泛型方法定义绑定类型实参，返回构造后的 MethodInfo。本方法非泛型定义或实参个数不符时抛 `Std.Exception`。

