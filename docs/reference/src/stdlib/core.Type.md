<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Type

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Type.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L13)

运行期类型描述符，由 `Object.GetType()` 返回（C# System.Type 对标）。

携带 native 句柄指向真实 VM `TypeDesc`；基元（int / long / bool / …）解析到其真实
`Std.*` 包装结构体句柄——`typeof(int)` 与 `(5).GetType()` **值相等**（按 FullName，见
`operator ==`），Name 为 `Int32`、FullName 为 `Std.Int32`、IsValueType true、成员可枚举。
只有数组返回无句柄的合成体（Name/FullName 已填，成员查询为空）。不能从用户代码构造。

注：「相等」是值相等（按 FullName），不是同一个对象——每次 typeof / GetType() 都新建，
`ReferenceEquals` 恒为假。

```z42
public sealed class Type : MemberInfo {
    string __name
    string __fullName
    bool IsArray
    string __elementName
    long __asmId
    Type[] __typeArgs
    static extern Type GetType(string fqn)
    extern Type GetElementType()
    extern string FullName { get; }
    static bool op_Equality(Type a, Type b)
    static bool op_Inequality(Type a, Type b)
    override bool Equals(object other)
    override int GetHashCode()
    extern Type BaseType { get; }
    extern bool IsAbstract { get; }
    extern bool IsSealed { get; }
    extern bool IsValueType { get; }
    extern bool IsRecord { get; }
    extern bool IsInterface { get; }
    extern bool IsEnum { get; }
    extern Type GetEnumUnderlyingType()
    extern bool IsNested { get; }
    extern Type GetDeclaringType()
    extern Type[] GetNestedTypes()
    extern bool IsDelegate { get; }
    extern bool IsClass { get; }
    extern bool IsGenericType { get; }
    extern bool IsGenericTypeDefinition { get; }
    extern bool IsPrimitive { get; }
    extern TypeVisibility Visibility { get; }
    bool IsPublic { get; }
    bool IsNotPublic { get; }
    bool IsNestedPublic { get; }
    bool IsNestedPrivate { get; }
    bool IsNestedFamily { get; }
    bool IsNestedAssembly { get; }
    extern Std.Reflection.FieldInfo[] GetFields()
    extern Std.Reflection.MethodInfo[] GetMethods()
    extern Std.Reflection.ConstructorInfo[] GetConstructors()
    extern Std.Reflection.MemberInfo[] GetMembers()
    extern Std.Reflection.PropertyInfo[] GetProperties()
    extern Type[] GetGenericArguments()
    extern Type GetGenericTypeDefinition()
    extern Type MakeGenericType(params Type[] typeArgs)
    extern Type[] GetInterfaces()
    Type? GetInterface(string name)
    extern bool IsAssignableFrom(Type c)
    extern bool IsCollectible { get; }
    extern Std.Reflection.Assembly Assembly { get; }
    Attribute[] __attrCache
    extern Attribute[] __customAttributes()
    Attribute[] GetCustomAttributes()
    Attribute? GetAttribute(Type attributeType)
}
```

成员：[`__name`](#Std.Type.__name) · [`__fullName`](#Std.Type.__fullName) · [`IsArray`](#Std.Type.IsArray) · [`__elementName`](#Std.Type.__elementName) · [`__asmId`](#Std.Type.__asmId) · [`__typeArgs`](#Std.Type.__typeArgs) · [`GetType`](#Std.Type.GetType(string)) · [`GetElementType`](#Std.Type.GetElementType()) · [`FullName`](#Std.Type.FullName) · [`op_Equality`](#Std.Type.op_Equality(Type,Type)) · [`op_Inequality`](#Std.Type.op_Inequality(Type,Type)) · [`Equals`](#Std.Type.Equals(object)) · [`GetHashCode`](#Std.Type.GetHashCode()) · [`BaseType`](#Std.Type.BaseType) · [`IsAbstract`](#Std.Type.IsAbstract) · [`IsSealed`](#Std.Type.IsSealed) · [`IsValueType`](#Std.Type.IsValueType) · [`IsRecord`](#Std.Type.IsRecord) · [`IsInterface`](#Std.Type.IsInterface) · [`IsEnum`](#Std.Type.IsEnum) · [`GetEnumUnderlyingType`](#Std.Type.GetEnumUnderlyingType()) · [`IsNested`](#Std.Type.IsNested) · [`GetDeclaringType`](#Std.Type.GetDeclaringType()) · [`GetNestedTypes`](#Std.Type.GetNestedTypes()) · [`IsDelegate`](#Std.Type.IsDelegate) · [`IsClass`](#Std.Type.IsClass) · [`IsGenericType`](#Std.Type.IsGenericType) · [`IsGenericTypeDefinition`](#Std.Type.IsGenericTypeDefinition) · [`IsPrimitive`](#Std.Type.IsPrimitive) · [`Visibility`](#Std.Type.Visibility) · [`IsPublic`](#Std.Type.IsPublic) · [`IsNotPublic`](#Std.Type.IsNotPublic) · [`IsNestedPublic`](#Std.Type.IsNestedPublic) · [`IsNestedPrivate`](#Std.Type.IsNestedPrivate) · [`IsNestedFamily`](#Std.Type.IsNestedFamily) · [`IsNestedAssembly`](#Std.Type.IsNestedAssembly) · [`GetFields`](#Std.Type.GetFields()) · [`GetMethods`](#Std.Type.GetMethods()) · [`GetConstructors`](#Std.Type.GetConstructors()) · [`GetMembers`](#Std.Type.GetMembers()) · [`GetProperties`](#Std.Type.GetProperties()) · [`GetGenericArguments`](#Std.Type.GetGenericArguments()) · [`GetGenericTypeDefinition`](#Std.Type.GetGenericTypeDefinition()) · [`MakeGenericType`](#Std.Type.MakeGenericType(Type[])) · [`GetInterfaces`](#Std.Type.GetInterfaces()) · [`GetInterface`](#Std.Type.GetInterface(string)) · [`IsAssignableFrom`](#Std.Type.IsAssignableFrom(Type)) · [`IsCollectible`](#Std.Type.IsCollectible) · [`Assembly`](#Std.Type.Assembly) · [`__attrCache`](#Std.Type.__attrCache) · [`__customAttributes`](#Std.Type.__customAttributes()) · [`GetCustomAttributes`](#Std.Type.GetCustomAttributes()) · [`GetAttribute`](#Std.Type.GetAttribute(Type))

<a id="Std.Type.__name"></a>
### string __name

[`src/libraries/z42.core/src/Type.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L15)

VM 写入的短名（优先用继承的 `Name` 属性；此处保留给底层 golden 测试与 z42.harness 直读）。

<a id="Std.Type.__fullName"></a>
### string __fullName

[`src/libraries/z42.core/src/Type.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L17)

VM 写入的全限定名（优先用 `FullName` 属性）。

<a id="Std.Type.IsArray"></a>
### bool IsArray

[`src/libraries/z42.core/src/Type.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L21)

VM 写入：数组类型（`typeof(int[])`、数组字段/参数类型、`arr.GetType()`）为 true。 `__elementName` 存元素类型的 FQ 名；`GetElementType()` 解析为 `Type`（缺省 null）。

<a id="Std.Type.__elementName"></a>
### string __elementName

[`src/libraries/z42.core/src/Type.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L23)

数组元素类型的 FQ 名（非数组为 ""）。

<a id="Std.Type.__asmId"></a>
### long __asmId

[`src/libraries/z42.core/src/Type.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L26)

VM 写入：定义该类型的 assembly 的 AssemblyId（0 = root）。内部（`__` 前缀），非用户读取。

<a id="Std.Type.__typeArgs"></a>
### Type[] __typeArgs

[`src/libraries/z42.core/src/Type.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L31)

VM 写入：构造泛型（`typeof(Box<int>)`）的实参 `Type[]`；开放定义与非泛型为 null。 非 null 标记该类型为构造类型，故 `IsGenericTypeDefinition` false、 `GetGenericArguments()` 返回这些。

<a id="Std.Type.GetType(string)"></a>
### static extern Type GetType(string fqn)

[`src/libraries/z42.core/src/Type.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L36)

按全限定名解析 Type（如 `"Demo.Point"`）；未知返回 null。补足 typeof(T) / obj.GetType() 之外的反射查找路径。

<a id="Std.Type.GetElementType()"></a>
### extern Type GetElementType()

[`src/libraries/z42.core/src/Type.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L40)

数组类型的元素类型（`typeof(int[]).GetElementType()` → `typeof(int)`）；非数组为 null。

<a id="Std.Type.FullName"></a>
### extern string FullName { get; }

[`src/libraries/z42.core/src/Type.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L44)

含命名空间的全限定名，如 `"geometry.Circle"`。

<a id="Std.Type.op_Equality(Type,Type)"></a>
### static bool op_Equality(Type a, Type b)

[`src/libraries/z42.core/src/Type.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L52)

按 FullName 比较相等（两侧皆 null 才相等；一侧 null 为不等）。

<a id="Std.Type.op_Inequality(Type,Type)"></a>
### static bool op_Inequality(Type a, Type b)

[`src/libraries/z42.core/src/Type.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L60)

按 FullName 判断不等。

<a id="Std.Type.Equals(object)"></a>
### override bool Equals(object other)

[`src/libraries/z42.core/src/Type.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L68)

与 `==` 同一判据（FullName），保证与 GetHashCode 口径一致。

<a id="Std.Type.GetHashCode()"></a>
### override int GetHashCode()

[`src/libraries/z42.core/src/Type.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L75)

FullName 的哈希码。

<a id="Std.Type.BaseType"></a>
### extern Type BaseType { get; }

[`src/libraries/z42.core/src/Type.z42:79`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L79)

基类类型；无显式基类的类为 `Std.Object`；`Std.Object` 自身与无句柄 Type（基元/数组）为 null。

<a id="Std.Type.IsAbstract"></a>
### extern bool IsAbstract { get; }

[`src/libraries/z42.core/src/Type.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L83)

是否为 `abstract` 类型。非 abstract 类与无句柄 Type（基元/数组）为 false。

<a id="Std.Type.IsSealed"></a>
### extern bool IsSealed { get; }

[`src/libraries/z42.core/src/Type.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L87)

是否为 `sealed` 类型。否则 / 无句柄为 false。

<a id="Std.Type.IsValueType"></a>
### extern bool IsValueType { get; }

[`src/libraries/z42.core/src/Type.z42:91`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L91)

是否为值类型（`struct`）。

<a id="Std.Type.IsRecord"></a>
### extern bool IsRecord { get; }

[`src/libraries/z42.core/src/Type.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L95)

是否为 `record` 类型。

<a id="Std.Type.IsInterface"></a>
### extern bool IsInterface { get; }

[`src/libraries/z42.core/src/Type.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L99)

是否为 `interface`。class / struct / record / enum / 基元 / 数组为 false。

<a id="Std.Type.IsEnum"></a>
### extern bool IsEnum { get; }

[`src/libraries/z42.core/src/Type.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L103)

是否为 `enum`。

<a id="Std.Type.GetEnumUnderlyingType()"></a>
### extern Type GetEnumUnderlyingType()

[`src/libraries/z42.core/src/Type.z42:108`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L108)

enum 的底层整型。z42 一律以 i64（long）背书 enum → 恒返 typeof(long)。 非 enum 抛 Std.Exception（镜像 C#）。

<a id="Std.Type.IsNested"></a>
### extern bool IsNested { get; }

[`src/libraries/z42.core/src/Type.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L116)

是否嵌套于另一类型（其 FQ 名含 `+`）。

<a id="Std.Type.GetDeclaringType()"></a>
### extern Type GetDeclaringType()

[`src/libraries/z42.core/src/Type.z42:120`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L120)

直接外层类型（嵌套时），否则 null。`typeof(Outer.Inner).GetDeclaringType()` → typeof(Outer)。

<a id="Std.Type.GetNestedTypes()"></a>
### extern Type[] GetNestedTypes()

[`src/libraries/z42.core/src/Type.z42:124`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L124)

直接嵌套于本类型的类型（不含更深层 / 继承），有序。

<a id="Std.Type.IsDelegate"></a>
### extern bool IsDelegate { get; }

[`src/libraries/z42.core/src/Type.z42:128`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L128)

是否为 `delegate` 类型（签名经 GetMethods() 的合成 Invoke 反射）。

<a id="Std.Type.IsClass"></a>
### extern bool IsClass { get; }

[`src/libraries/z42.core/src/Type.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L132)

是否为引用类类型（含 `record`）。既非值类型（struct）也非接口；基元/数组/enum 为 false。

<a id="Std.Type.IsGenericType"></a>
### extern bool IsGenericType { get; }

[`src/libraries/z42.core/src/Type.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L136)

是否为泛型类型（含类型参数，如 `Box<T>`）。

<a id="Std.Type.IsGenericTypeDefinition"></a>
### extern bool IsGenericTypeDefinition { get; }

[`src/libraries/z42.core/src/Type.z42:140`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L140)

是否为泛型**定义**（开放 `Box<>`）而非构造实例化（`Box<int>`）。非泛型为 false。

<a id="Std.Type.IsPrimitive"></a>
### extern bool IsPrimitive { get; }

[`src/libraries/z42.core/src/Type.z42:144`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L144)

是否为基元类型（int / long / bool / char / float / double / ...）。`string` 不是基元。

<a id="Std.Type.Visibility"></a>
### extern TypeVisibility Visibility { get; }

[`src/libraries/z42.core/src/Type.z42:152`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L152)

类型的声明访问级（public / private / protected / internal）。无句柄 Type → Public。

<a id="Std.Type.IsPublic"></a>
### bool IsPublic { get; }

[`src/libraries/z42.core/src/Type.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L159)

顶层 public 类型（非嵌套且 public）。

<a id="Std.Type.IsNotPublic"></a>
### bool IsNotPublic { get; }

[`src/libraries/z42.core/src/Type.z42:161`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L161)

顶层非 public 类型（非嵌套且非 public，即顶层 internal）。

<a id="Std.Type.IsNestedPublic"></a>
### bool IsNestedPublic { get; }

[`src/libraries/z42.core/src/Type.z42:163`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L163)

嵌套 public。

<a id="Std.Type.IsNestedPrivate"></a>
### bool IsNestedPrivate { get; }

[`src/libraries/z42.core/src/Type.z42:165`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L165)

嵌套 private。

<a id="Std.Type.IsNestedFamily"></a>
### bool IsNestedFamily { get; }

[`src/libraries/z42.core/src/Type.z42:167`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L167)

嵌套 protected（C# "family"）。

<a id="Std.Type.IsNestedAssembly"></a>
### bool IsNestedAssembly { get; }

[`src/libraries/z42.core/src/Type.z42:169`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L169)

嵌套 internal（C# "assembly"）。

<a id="Std.Type.GetFields()"></a>
### extern Std.Reflection.FieldInfo[] GetFields()

[`src/libraries/z42.core/src/Type.z42:173`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L173)

公开实例字段（含继承；基类在前）。基元 / 数组为空。

<a id="Std.Type.GetMethods()"></a>
### extern Std.Reflection.MethodInfo[] GetMethods()

[`src/libraries/z42.core/src/Type.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L177)

方法（含继承 / 虚方法）。

<a id="Std.Type.GetConstructors()"></a>
### extern Std.Reflection.ConstructorInfo[] GetConstructors()

[`src/libraries/z42.core/src/Type.z42:182`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L182)

本类型声明的构造器。每个 ConstructorInfo.Invoke(args) 分配并构造新实例。基元 / 数组 / 接口为空。按参数类型的重载选择（GetConstructor(Type[])）未实现——枚举后自行查参数。

<a id="Std.Type.GetMembers()"></a>
### extern Std.Reflection.MemberInfo[] GetMembers()

[`src/libraries/z42.core/src/Type.z42:186`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L186)

字段与方法合在一起。

<a id="Std.Type.GetProperties()"></a>
### extern Std.Reflection.PropertyInfo[] GetProperties()

[`src/libraries/z42.core/src/Type.z42:191`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L191)

属性（auto-property + extern getter），由 `get_<X>` / `set_<X>` 访问器约定派生，含继承。 基元 / 数组为空。访问器方法仍出现在 GetMethods() 中（C# 对齐）；这是附加的属性视图。

<a id="Std.Type.GetGenericArguments()"></a>
### extern Type[] GetGenericArguments()

[`src/libraries/z42.core/src/Type.z42:195`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L195)

构造泛型的实参（`typeof(Box<int>)` → [int]）。开放定义与非泛型为空。

<a id="Std.Type.GetGenericTypeDefinition()"></a>
### extern Type GetGenericTypeDefinition()

[`src/libraries/z42.core/src/Type.z42:200`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L200)

构造类型的开放泛型定义（`typeof(Box<int>).GetGenericTypeDefinition()` → `Box<>`）。 非泛型抛异常（镜像 C# InvalidOperationException）。

<a id="Std.Type.MakeGenericType(Type[])"></a>
### extern Type MakeGenericType(params Type[] typeArgs)

[`src/libraries/z42.core/src/Type.z42:206`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L206)

由开放定义 + 实参在运行期构造泛型（`typeof(List<>).MakeGenericType(typeof(int))` → `List<int>`）。校验实参个数与 `where` 约束，违反抛 Std.Exception（反射是绕过编译期 约束检查的逃生门，故自行把关）。

<a id="Std.Type.GetInterfaces()"></a>
### extern Type[] GetInterfaces()

[`src/libraries/z42.core/src/Type.z42:211`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L211)

本类型实现的接口（自身声明 + 基类链继承，按名去重）。返回真实接口句柄 （IsInterface / FullName 可用）。接口的传递继承（接口继承接口）尚未包含。

<a id="Std.Type.GetInterface(string)"></a>
### Type? GetInterface(string name)

[`src/libraries/z42.core/src/Type.z42:214`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L214)

本类型实现的、给定（简单）名的接口，否则 null。GetInterfaces() 的便利封装。

<a id="Std.Type.IsAssignableFrom(Type)"></a>
### extern bool IsAssignableFrom(Type c)

[`src/libraries/z42.core/src/Type.z42:230`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L230)

`c` 的实例能否赋给本类型变量：`c` 是 this、派生自 this、或实现接口 `this`。 反射形式的 `is`；VM 里比真实类型身份（非字符串）。null → false。

<a id="Std.Type.IsCollectible"></a>
### extern bool IsCollectible { get; }

[`src/libraries/z42.core/src/Type.z42:237`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L237)

是否加载进可回收的 AssemblyLoadContext（镜像 .NET Type.IsCollectible）。 常驻 root context 的类型（core/stdlib/main 与基元/数组合成体）→ false。

<a id="Std.Type.Assembly"></a>
### extern Std.Reflection.Assembly Assembly { get; }

[`src/libraries/z42.core/src/Type.z42:241`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L241)

定义此类型的 assembly（zpkg 运行期投影，镜像 .NET Type.Assembly）。root 类型报合成 root。

<a id="Std.Type.__attrCache"></a>
### Attribute[] __attrCache

[`src/libraries/z42.core/src/Type.z42:247`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L247)

本类型 attribute 实例的惰性缓存：首次 GetCustomAttributes() 前为 null，之后同一数组 （同一批实例）复用（C# 每次调用都新分配，z42 不）。

<a id="Std.Type.__customAttributes()"></a>
### extern Attribute[] __customAttributes()

[`src/libraries/z42.core/src/Type.z42:251`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L251)

VM 构建的活 attribute 实例（每个应用一次，按应用顺序）。VM 调用各自编译器合成工厂。

<a id="Std.Type.GetCustomAttributes()"></a>
### Attribute[] GetCustomAttributes()

[`src/libraries/z42.core/src/Type.z42:254`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L254)

应用于本类型的用户 attribute（活实例）。缓存：首次调用物化，重复调用返回同一批实例。

<a id="Std.Type.GetAttribute(Type)"></a>
### Attribute? GetAttribute(Type attributeType)

[`src/libraries/z42.core/src/Type.z42:264`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Type.z42#L264)

给定类型的单个 attribute，否则 null。按实例运行时类型（FullName）比较，子类 attribute 也命中。

