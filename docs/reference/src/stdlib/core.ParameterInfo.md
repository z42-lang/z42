<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ParameterInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:5`

反射方法参数，对标 C# `System.Reflection.ParameterInfo`（只读子集），由 `MethodInfo.GetParameters()` 填充。

```z42
public class ParameterInfo {
    string Name
    Std.Type ParameterType
    int Position
    bool IsOptional
    bool IsParams
    object? DefaultValue
    string __qualified
    Std.Attribute[] __attrCache
    extern Std.Attribute[] __customAttributes(string qualified, int position)
    Std.Attribute[] GetCustomAttributes()
    Std.Attribute? GetAttribute(Std.Type attributeType)
}
```

### string Name

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:7`

参数名。

### Std.Type ParameterType

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:10`

参数的声明类型，如 `typeof(int)`。

### int Position

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:13`

在参数列表中的 0 基位置（不含隐式 `this`）。

### bool IsOptional

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:16`

该参数是否有默认值。

### bool IsParams

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:18`

该参数是否为 `params` 变长参数。

### object? DefaultValue

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:22`

折出的常量默认值（字面量与常量表达式）；折不出的为 null。

注意：`DefaultValue == null` 不代表没有默认值，判有无看 `IsOptional`。

### string __qualified

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:27`


### Std.Attribute[] __attrCache

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:30`


### extern Std.Attribute[] __customAttributes(string qualified, int position)

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:33`


### Std.Attribute[] GetCustomAttributes()

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:36`

本参数上应用的 attribute，以活实例返回；首次调用后缓存。

### Std.Attribute? GetAttribute(Std.Type attributeType)

`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:45`

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

