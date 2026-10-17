<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MethodInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:8`

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

### Std.Type ReturnType

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:10`

返回类型，如 `typeof(int)` / `typeof(void)`。VM 写入。

### bool IsVirtual

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:15`

是否为 virtual / override / abstract 方法（三者都走虚派发）。

### bool IsAbstract

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:18`

是否为 abstract 方法（同时 IsVirtual == true）。

### bool IsSealed

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:21`

是否为 `sealed override`（或简写 `sealed`）方法——不允许再 override（同时 IsVirtual == true）。

### bool IsPublic

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:24`

声明为 public。

### bool IsPrivate

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:26`

声明为 private。`protected` 方法两者皆 false（镜像 C#）。

### static bool op_Equality(MethodInfo a, MethodInfo b)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:34`

按 `__qualified`（全限定方法名）判断两个 `MethodInfo` 是否指向同一方法。

### static bool op_Inequality(MethodInfo a, MethodInfo b)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:42`

不等运算符，等价于 `!(a == b)`。

### override bool Equals(object other)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:49`


### override int GetHashCode()

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:55`


### Std.Attribute[] __attrCache

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:60`


### extern Std.Attribute[] __customAttributes(string qualified)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:63`


### Std.Attribute[] GetCustomAttributes()

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:66`

本方法上应用的 attribute，以活实例返回；首次调用后缓存。

### Std.Attribute? GetAttribute(Std.Type attributeType)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:75`

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

### extern object Invoke(object obj, object[] args)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:93`

调用本方法：实例方法 `obj` 是接收者（静态方法传 `null` 即可）；`args` 按声明序映射形参。

返回方法返回值（`void` → null）；被调方法内的 `throw` 以原类型传播。仅限非泛型方法。

### bool IsGenericMethod

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:98`

方法是否声明了类型参数（定义或已构造均为 true）。

### bool IsGenericMethodDefinition

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:101`

是否为未绑定的泛型方法定义；经 `MakeGenericMethod` 绑定实参后为 false。

### string[] __typeParamNames

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:104`


### Std.Type[] __typeArgs

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:107`


### extern Std.Type[] GetGenericArguments()

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:111`

方法的类型参数（定义上）或绑定的类型实参（构造后）。

### extern MethodInfo MakeGenericMethod(params Std.Type[] typeArgs)

`src/libraries/z42.core/src/Reflection/MethodInfo.z42:116`

给泛型方法定义绑定类型实参，返回构造后的 MethodInfo。本方法非泛型定义或实参个数不符时抛 `Std.Exception`。

