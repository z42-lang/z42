<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.PropertyInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:8`

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

### Std.Type PropertyType

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:10`

属性类型——getter 的返回类型；只写属性取 setter 的值形参类型。

### bool CanRead

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:13`

是否有 getter（`get_<Name>`）。

### bool CanWrite

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:16`

是否有 setter（`set_<Name>`）。

### string __getterQualified

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:19`


### string __setterQualified

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:20`


### extern object GetValue(object obj)

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:24`

经 getter 从 `obj` 读出本属性值。只写属性（无 getter）抛 `Std.Exception`。

### extern void SetValue(object obj, object value)

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:28`

经 setter 把 `value` 写入 `obj` 上的本属性。只读属性（无 setter）抛 `Std.Exception`。

### Std.Attribute[] __attrCache

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:35`


### extern Std.Attribute[] __customAttributes(string qualified)

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:38`


### Std.Attribute[] GetCustomAttributes()

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:41`

本属性上应用的 attribute，以活实例返回；首次调用后缓存。用访问器全限定名（getter 优先，否则 setter）解析。

### Std.Attribute? GetAttribute(Std.Type attributeType)

`src/libraries/z42.core/src/Reflection/PropertyInfo.z42:56`

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

