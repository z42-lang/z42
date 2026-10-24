<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ParameterInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L6)

反射方法参数，对标 C# `System.Reflection.ParameterInfo`（只读子集）。

由 `MethodInfo.GetParameters()` 填充。

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

成员：[`Name`](#Std.Reflection.ParameterInfo.Name) · [`ParameterType`](#Std.Reflection.ParameterInfo.ParameterType) · [`Position`](#Std.Reflection.ParameterInfo.Position) · [`IsOptional`](#Std.Reflection.ParameterInfo.IsOptional) · [`IsParams`](#Std.Reflection.ParameterInfo.IsParams) · [`DefaultValue`](#Std.Reflection.ParameterInfo.DefaultValue) · [`__qualified`](#Std.Reflection.ParameterInfo.__qualified) · [`__attrCache`](#Std.Reflection.ParameterInfo.__attrCache) · [`__customAttributes`](#Std.Reflection.ParameterInfo.__customAttributes(string,int)) · [`GetCustomAttributes`](#Std.Reflection.ParameterInfo.GetCustomAttributes()) · [`GetAttribute`](#Std.Reflection.ParameterInfo.GetAttribute(Std.Type))

<a id="Std.Reflection.ParameterInfo.Name"></a>
### string Name

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L8)

参数名。

<a id="Std.Reflection.ParameterInfo.ParameterType"></a>
### Std.Type ParameterType

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L11)

参数的声明类型，如 `typeof(int)`。

<a id="Std.Reflection.ParameterInfo.Position"></a>
### int Position

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L14)

在参数列表中的 0 基位置（不含隐式 `this`）。

<a id="Std.Reflection.ParameterInfo.IsOptional"></a>
### bool IsOptional

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L17)

该参数是否有默认值。

<a id="Std.Reflection.ParameterInfo.IsParams"></a>
### bool IsParams

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L19)

该参数是否为 `params` 变长参数。

<a id="Std.Reflection.ParameterInfo.DefaultValue"></a>
### object? DefaultValue

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L23)

折出的常量默认值（字面量与常量表达式）；折不出的为 null。

注意：`DefaultValue == null` 不代表没有默认值，判有无看 `IsOptional`。

<a id="Std.Reflection.ParameterInfo.__qualified"></a>
### string __qualified

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L28)

后备方法/函数的全限定名（VM 写入），与 `Position` 一起用于解析本参数的 attribute 工厂。

<a id="Std.Reflection.ParameterInfo.__attrCache"></a>
### Std.Attribute[] __attrCache

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L31)

缓存的 attribute 实例（缓存理由见 `Type.GetCustomAttributes`）。

<a id="Std.Reflection.ParameterInfo.__customAttributes(string,int)"></a>
### extern Std.Attribute[] __customAttributes(string qualified, int position)

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L35)

按全限定名与参数位置取 attribute 定义的 native 通道。

<a id="Std.Reflection.ParameterInfo.GetCustomAttributes()"></a>
### Std.Attribute[] GetCustomAttributes()

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L38)

本参数上应用的 attribute，以活实例返回；首次调用后缓存。

<a id="Std.Reflection.ParameterInfo.GetAttribute(Std.Type)"></a>
### Std.Attribute? GetAttribute(Std.Type attributeType)

[`src/libraries/z42.core/src/Reflection/ParameterInfo.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ParameterInfo.z42#L47)

指定类型的单个 attribute，无则返回 null。按实例运行期类型的 FullName 比较，子类 attribute 也匹配。

