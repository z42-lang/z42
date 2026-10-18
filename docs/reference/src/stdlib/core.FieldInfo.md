<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.FieldInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L7)

反射字段，对标 C# `System.Reflection.FieldInfo`，由 `Type.GetFields()` 填充。

`GetValue` / `SetValue` 直接读写实例字段的槽（字段即槽、无访问器），是反射式（反）序列化落到
公开字段上的通路。

```z42
public class FieldInfo : MemberInfo {
    Std.Type FieldType
    bool IsStatic
    extern object GetValue(object obj)
    extern void SetValue(object obj, object value)
    bool IsPublic
    bool IsPrivate
    string __qualified
    Std.Attribute[] __attrCache
    extern Std.Attribute[] __customAttributes(string qualified)
    Std.Attribute[] GetCustomAttributes()
    Std.Attribute? GetAttribute(Std.Type attributeType)
}
```

成员：[`FieldType`](#Std.Reflection.FieldInfo.FieldType) · [`IsStatic`](#Std.Reflection.FieldInfo.IsStatic) · [`GetValue`](#Std.Reflection.FieldInfo.GetValue(object)) · [`SetValue`](#Std.Reflection.FieldInfo.SetValue(object,object)) · [`IsPublic`](#Std.Reflection.FieldInfo.IsPublic) · [`IsPrivate`](#Std.Reflection.FieldInfo.IsPrivate) · [`__qualified`](#Std.Reflection.FieldInfo.__qualified) · [`__attrCache`](#Std.Reflection.FieldInfo.__attrCache) · [`__customAttributes`](#Std.Reflection.FieldInfo.__customAttributes(string)) · [`GetCustomAttributes`](#Std.Reflection.FieldInfo.GetCustomAttributes()) · [`GetAttribute`](#Std.Reflection.FieldInfo.GetAttribute(Std.Type))

<a id="Std.Reflection.FieldInfo.FieldType"></a>
### Std.Type FieldType

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L9)

字段的声明类型，如 `typeof(int)`。VM 写入。

<a id="Std.Reflection.FieldInfo.IsStatic"></a>
### bool IsStatic

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L12)

是否为静态字段。

<a id="Std.Reflection.FieldInfo.GetValue(object)"></a>
### extern object GetValue(object obj)

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L16)

从 `obj` 读出本实例字段的值。

<a id="Std.Reflection.FieldInfo.SetValue(object,object)"></a>
### extern void SetValue(object obj, object value)

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L20)

把 `value` 写入 `obj` 上的本实例字段。

<a id="Std.Reflection.FieldInfo.IsPublic"></a>
### bool IsPublic

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L23)

声明为 public。

<a id="Std.Reflection.FieldInfo.IsPrivate"></a>
### bool IsPrivate

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L25)

声明为 private。`protected` 字段两者皆 false（镜像 C#）。

<a id="Std.Reflection.FieldInfo.__qualified"></a>
### string __qualified

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L30)


<a id="Std.Reflection.FieldInfo.__attrCache"></a>
### Std.Attribute[] __attrCache

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L33)


<a id="Std.Reflection.FieldInfo.__customAttributes(string)"></a>
### extern Std.Attribute[] __customAttributes(string qualified)

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L36)


<a id="Std.Reflection.FieldInfo.GetCustomAttributes()"></a>
### Std.Attribute[] GetCustomAttributes()

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L39)

本字段上应用的 attribute，以活实例返回；首次调用后缓存。

<a id="Std.Reflection.FieldInfo.GetAttribute(Std.Type)"></a>
### Std.Attribute? GetAttribute(Std.Type attributeType)

[`src/libraries/z42.core/src/Reflection/FieldInfo.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/FieldInfo.z42#L48)

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

