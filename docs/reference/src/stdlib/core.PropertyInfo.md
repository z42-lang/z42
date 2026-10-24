<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.PropertyInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L8)

反射属性，对标 C# `System.Reflection.PropertyInfo`，由 `Type.GetProperties()` 填充。

由 `get_<X>` / `set_<X>` 访问器方法约定在运行时派生（auto-property 降解为字段 + get_/set_ 方法）：
getter 使其可读、setter 使其可写，同名 getter + setter 合并为一条。`GetValue` / `SetValue` 经非泛型
invoke 路径反射调用访问器。

```z42
public class PropertyInfo : MemberInfo {
    Std.Type PropertyType
    bool CanRead
    bool CanWrite
    string __getterQualified
    string __setterQualified
    extern object GetValue(object obj)
    extern void SetValue(object obj, object value)
    Std.Attribute[] __attrCache
    extern Std.Attribute[] __customAttributes(string qualified)
    Std.Attribute[] GetCustomAttributes()
    Std.Attribute? GetAttribute(Std.Type attributeType)
}
```

成员：[`PropertyType`](#Std.Reflection.PropertyInfo.PropertyType) · [`CanRead`](#Std.Reflection.PropertyInfo.CanRead) · [`CanWrite`](#Std.Reflection.PropertyInfo.CanWrite) · [`__getterQualified`](#Std.Reflection.PropertyInfo.__getterQualified) · [`__setterQualified`](#Std.Reflection.PropertyInfo.__setterQualified) · [`GetValue`](#Std.Reflection.PropertyInfo.GetValue(object)) · [`SetValue`](#Std.Reflection.PropertyInfo.SetValue(object,object)) · [`__attrCache`](#Std.Reflection.PropertyInfo.__attrCache) · [`__customAttributes`](#Std.Reflection.PropertyInfo.__customAttributes(string)) · [`GetCustomAttributes`](#Std.Reflection.PropertyInfo.GetCustomAttributes()) · [`GetAttribute`](#Std.Reflection.PropertyInfo.GetAttribute(Std.Type))

<a id="Std.Reflection.PropertyInfo.PropertyType"></a>
### Std.Type PropertyType

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L10)

属性类型——getter 的返回类型；只写属性取 setter 的值形参类型。

<a id="Std.Reflection.PropertyInfo.CanRead"></a>
### bool CanRead

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L13)

是否有 getter（`get_<Name>`）。

<a id="Std.Reflection.PropertyInfo.CanWrite"></a>
### bool CanWrite

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L16)

是否有 setter（`set_<Name>`）。

<a id="Std.Reflection.PropertyInfo.__getterQualified"></a>
### string __getterQualified

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L21)

getter（`get_<Name>`）的全限定名（VM 在 __type_properties 里盖章）；无 getter 时为 null。

GetValue 据此解析要调的函数。

<a id="Std.Reflection.PropertyInfo.__setterQualified"></a>
### string __setterQualified

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L25)

setter（`set_<Name>`）的全限定名；无 setter 时为 null。

SetValue 据此解析要调的函数。

<a id="Std.Reflection.PropertyInfo.GetValue(object)"></a>
### extern object GetValue(object obj)

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L29)

经 getter 从 `obj` 读出本属性值。只写属性（无 getter）抛 `Std.Exception`。

<a id="Std.Reflection.PropertyInfo.SetValue(object,object)"></a>
### extern void SetValue(object obj, object value)

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L33)

经 setter 把 `value` 写入 `obj` 上的本属性。只读属性（无 setter）抛 `Std.Exception`。

<a id="Std.Reflection.PropertyInfo.__attrCache"></a>
### Std.Attribute[] __attrCache

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L40)

缓存的 attribute 实例（缓存理由见 FieldInfo.GetCustomAttributes）。

<a id="Std.Reflection.PropertyInfo.__customAttributes(string)"></a>
### extern Std.Attribute[] __customAttributes(string qualified)

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L44)

native 门面：按访问器全限定名取本属性上的 attribute 实例数组。

<a id="Std.Reflection.PropertyInfo.GetCustomAttributes()"></a>
### Std.Attribute[] GetCustomAttributes()

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L47)

本属性上应用的 attribute，以活实例返回；首次调用后缓存。用访问器全限定名（getter 优先，否则 setter）解析。

<a id="Std.Reflection.PropertyInfo.GetAttribute(Std.Type)"></a>
### Std.Attribute? GetAttribute(Std.Type attributeType)

[`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/PropertyInfo.z42#L62)

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

