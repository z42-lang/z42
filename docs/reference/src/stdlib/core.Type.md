<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Type

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Type.z42:13`

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

### string __name

`src/libraries/z42.core/src/Type.z42:15`

VM 写入的短名（优先用继承的 `Name` 属性；此处保留给底层 golden 测试与 z42.harness 直读）。

### string __fullName

`src/libraries/z42.core/src/Type.z42:17`

VM 写入的全限定名（优先用 `FullName` 属性）。

### bool IsArray

`src/libraries/z42.core/src/Type.z42:21`

VM 写入：数组类型（`typeof(int[])`、数组字段/参数类型、`arr.GetType()`）为 true。 `__elementName` 存元素类型的 FQ 名；`GetElementType()` 解析为 `Type`（缺省 null）。

### string __elementName

`src/libraries/z42.core/src/Type.z42:23`

数组元素类型的 FQ 名（非数组为 ""）。

### long __asmId

`src/libraries/z42.core/src/Type.z42:26`

VM 写入：定义该类型的 assembly 的 AssemblyId（0 = root）。内部（`__` 前缀），非用户读取。

### Type[] __typeArgs

`src/libraries/z42.core/src/Type.z42:31`

VM 写入：构造泛型（`typeof(Box<int>)`）的实参 `Type[]`；开放定义与非泛型为 null。 非 null 标记该类型为构造类型，故 `IsGenericTypeDefinition` false、 `GetGenericArguments()` 返回这些。

### static extern Type GetType(string fqn)

`src/libraries/z42.core/src/Type.z42:36`

按全限定名解析 Type（如 `"Demo.Point"`）；未知返回 null。补足 typeof(T) / obj.GetType() 之外的反射查找路径。

### extern Type GetElementType()

`src/libraries/z42.core/src/Type.z42:40`

数组类型的元素类型（`typeof(int[]).GetElementType()` → `typeof(int)`）；非数组为 null。

### extern string FullName { get; }

`src/libraries/z42.core/src/Type.z42:44`

含命名空间的全限定名，如 `"geometry.Circle"`。

### static bool op_Equality(Type a, Type b)

`src/libraries/z42.core/src/Type.z42:52`

按 FullName 比较相等（两侧皆 null 才相等；一侧 null 为不等）。

### static bool op_Inequality(Type a, Type b)

`src/libraries/z42.core/src/Type.z42:60`

按 FullName 判断不等。

### override bool Equals(object other)

`src/libraries/z42.core/src/Type.z42:68`

与 `==` 同一判据（FullName），保证与 GetHashCode 口径一致。

### override int GetHashCode()

`src/libraries/z42.core/src/Type.z42:75`

FullName 的哈希码。

### extern Type BaseType { get; }

`src/libraries/z42.core/src/Type.z42:79`

基类类型；无显式基类的类为 `Std.Object`；`Std.Object` 自身与无句柄 Type（基元/数组）为 null。

### extern bool IsAbstract { get; }

`src/libraries/z42.core/src/Type.z42:83`

是否为 `abstract` 类型。非 abstract 类与无句柄 Type（基元/数组）为 false。

### extern bool IsSealed { get; }

`src/libraries/z42.core/src/Type.z42:87`

是否为 `sealed` 类型。否则 / 无句柄为 false。

### extern bool IsValueType { get; }

`src/libraries/z42.core/src/Type.z42:91`

是否为值类型（`struct`）。

### extern bool IsRecord { get; }

`src/libraries/z42.core/src/Type.z42:95`

是否为 `record` 类型。

### extern bool IsInterface { get; }

`src/libraries/z42.core/src/Type.z42:99`

是否为 `interface`。class / struct / record / enum / 基元 / 数组为 false。

### extern bool IsEnum { get; }

`src/libraries/z42.core/src/Type.z42:103`

是否为 `enum`。

### extern Type GetEnumUnderlyingType()

`src/libraries/z42.core/src/Type.z42:108`

enum 的底层整型。z42 一律以 i64（long）背书 enum → 恒返 typeof(long)。 非 enum 抛 Std.Exception（镜像 C#）。

### extern bool IsNested { get; }

`src/libraries/z42.core/src/Type.z42:116`

是否嵌套于另一类型（其 FQ 名含 `+`）。

### extern Type GetDeclaringType()

`src/libraries/z42.core/src/Type.z42:120`

直接外层类型（嵌套时），否则 null。`typeof(Outer.Inner).GetDeclaringType()` → typeof(Outer)。

### extern Type[] GetNestedTypes()

`src/libraries/z42.core/src/Type.z42:124`

直接嵌套于本类型的类型（不含更深层 / 继承），有序。

### extern bool IsDelegate { get; }

`src/libraries/z42.core/src/Type.z42:128`

是否为 `delegate` 类型（签名经 GetMethods() 的合成 Invoke 反射）。

### extern bool IsClass { get; }

`src/libraries/z42.core/src/Type.z42:132`

是否为引用类类型（含 `record`）。既非值类型（struct）也非接口；基元/数组/enum 为 false。

### extern bool IsGenericType { get; }

`src/libraries/z42.core/src/Type.z42:136`

是否为泛型类型（含类型参数，如 `Box<T>`）。

### extern bool IsGenericTypeDefinition { get; }

`src/libraries/z42.core/src/Type.z42:140`

是否为泛型**定义**（开放 `Box<>`）而非构造实例化（`Box<int>`）。非泛型为 false。

### extern bool IsPrimitive { get; }

`src/libraries/z42.core/src/Type.z42:144`

是否为基元类型（int / long / bool / char / float / double / ...）。`string` 不是基元。

### extern TypeVisibility Visibility { get; }

`src/libraries/z42.core/src/Type.z42:152`

类型的声明访问级（public / private / protected / internal）。无句柄 Type → Public。

### bool IsPublic { get; }

`src/libraries/z42.core/src/Type.z42:159`

顶层 public 类型（非嵌套且 public）。

### bool IsNotPublic { get; }

`src/libraries/z42.core/src/Type.z42:161`

顶层非 public 类型（非嵌套且非 public，即顶层 internal）。

### bool IsNestedPublic { get; }

`src/libraries/z42.core/src/Type.z42:163`

嵌套 public。

### bool IsNestedPrivate { get; }

`src/libraries/z42.core/src/Type.z42:165`

嵌套 private。

### bool IsNestedFamily { get; }

`src/libraries/z42.core/src/Type.z42:167`

嵌套 protected（C# "family"）。

### bool IsNestedAssembly { get; }

`src/libraries/z42.core/src/Type.z42:169`

嵌套 internal（C# "assembly"）。

### extern Std.Reflection.FieldInfo[] GetFields()

`src/libraries/z42.core/src/Type.z42:173`

公开实例字段（含继承；基类在前）。基元 / 数组为空。

### extern Std.Reflection.MethodInfo[] GetMethods()

`src/libraries/z42.core/src/Type.z42:177`

方法（含继承 / 虚方法）。

### extern Std.Reflection.ConstructorInfo[] GetConstructors()

`src/libraries/z42.core/src/Type.z42:182`

本类型声明的构造器。每个 ConstructorInfo.Invoke(args) 分配并构造新实例。基元 / 数组 / 接口为空。按参数类型的重载选择（GetConstructor(Type[])）未实现——枚举后自行查参数。

### extern Std.Reflection.MemberInfo[] GetMembers()

`src/libraries/z42.core/src/Type.z42:186`

字段与方法合在一起。

### extern Std.Reflection.PropertyInfo[] GetProperties()

`src/libraries/z42.core/src/Type.z42:191`

属性（auto-property + extern getter），由 `get_<X>` / `set_<X>` 访问器约定派生，含继承。 基元 / 数组为空。访问器方法仍出现在 GetMethods() 中（C# 对齐）；这是附加的属性视图。

### extern Type[] GetGenericArguments()

`src/libraries/z42.core/src/Type.z42:195`

构造泛型的实参（`typeof(Box<int>)` → [int]）。开放定义与非泛型为空。

### extern Type GetGenericTypeDefinition()

`src/libraries/z42.core/src/Type.z42:200`

构造类型的开放泛型定义（`typeof(Box<int>).GetGenericTypeDefinition()` → `Box<>`）。 非泛型抛异常（镜像 C# InvalidOperationException）。

### extern Type MakeGenericType(params Type[] typeArgs)

`src/libraries/z42.core/src/Type.z42:206`

由开放定义 + 实参在运行期构造泛型（`typeof(List<>).MakeGenericType(typeof(int))` → `List<int>`）。校验实参个数与 `where` 约束，违反抛 Std.Exception（反射是绕过编译期 约束检查的逃生门，故自行把关）。

### extern Type[] GetInterfaces()

`src/libraries/z42.core/src/Type.z42:211`

本类型实现的接口（自身声明 + 基类链继承，按名去重）。返回真实接口句柄 （IsInterface / FullName 可用）。接口的传递继承（接口继承接口）尚未包含。

### Type? GetInterface(string name)

`src/libraries/z42.core/src/Type.z42:214`

本类型实现的、给定（简单）名的接口，否则 null。GetInterfaces() 的便利封装。

### extern bool IsAssignableFrom(Type c)

`src/libraries/z42.core/src/Type.z42:230`

`c` 的实例能否赋给本类型变量：`c` 是 this、派生自 this、或实现接口 `this`。 反射形式的 `is`；VM 里比真实类型身份（非字符串）。null → false。

### extern bool IsCollectible { get; }

`src/libraries/z42.core/src/Type.z42:237`

是否加载进可回收的 AssemblyLoadContext（镜像 .NET Type.IsCollectible）。 常驻 root context 的类型（core/stdlib/main 与基元/数组合成体）→ false。

### extern Std.Reflection.Assembly Assembly { get; }

`src/libraries/z42.core/src/Type.z42:241`

定义此类型的 assembly（zpkg 运行期投影，镜像 .NET Type.Assembly）。root 类型报合成 root。

### Attribute[] __attrCache

`src/libraries/z42.core/src/Type.z42:247`

本类型 attribute 实例的惰性缓存：首次 GetCustomAttributes() 前为 null，之后同一数组 （同一批实例）复用（C# 每次调用都新分配，z42 不）。

### extern Attribute[] __customAttributes()

`src/libraries/z42.core/src/Type.z42:251`

VM 构建的活 attribute 实例（每个应用一次，按应用顺序）。VM 调用各自编译器合成工厂。

### Attribute[] GetCustomAttributes()

`src/libraries/z42.core/src/Type.z42:254`

应用于本类型的用户 attribute（活实例）。缓存：首次调用物化，重复调用返回同一批实例。

### Attribute? GetAttribute(Type attributeType)

`src/libraries/z42.core/src/Type.z42:264`

给定类型的单个 attribute，否则 null。按实例运行时类型（FullName）比较，子类 attribute 也命中。

