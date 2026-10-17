<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.FieldInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:7`

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

### Std.Type FieldType

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:9`

字段的声明类型，如 `typeof(int)`。VM 写入。

### bool IsStatic

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:12`

是否为静态字段。

### extern object GetValue(object obj)

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:16`

从 `obj` 读出本实例字段的值。

### extern void SetValue(object obj, object value)

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:20`

把 `value` 写入 `obj` 上的本实例字段。

### bool IsPublic

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:23`

声明为 public。

### bool IsPrivate

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:25`

声明为 private。`protected` 字段两者皆 false（镜像 C#）。

### string __qualified

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:30`


### Std.Attribute[] __attrCache

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:33`


### extern Std.Attribute[] __customAttributes(string qualified)

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:36`


### Std.Attribute[] GetCustomAttributes()

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:39`

本字段上应用的 attribute，以活实例返回；首次调用后缓存。

### Std.Attribute? GetAttribute(Std.Type attributeType)

`src/libraries/z42.core/src/Reflection/FieldInfo.z42:48`

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

