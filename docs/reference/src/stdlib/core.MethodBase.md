<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MethodBase

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/MethodBase.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodBase.z42#L8)

`MethodInfo` 与 `ConstructorInfo` 的公共基类，对标 C# `System.Reflection.MethodBase`。

持有方法与构造器共享的成员——静态性、限定后备函数名、参数列表。`Invoke` 由各子类定义：
`MethodInfo.Invoke` 调方法，`ConstructorInfo.Invoke` 分配并构造实例。按约定只读，仅由 VM
反射内建填充。

```z42
public class MethodBase : MemberInfo {
    bool IsStatic
    string __qualified
    ParameterInfo[] __parameters
    ParameterInfo[] GetParameters()
}
```

成员：[`IsStatic`](#Std.Reflection.MethodBase.IsStatic) · [`__qualified`](#Std.Reflection.MethodBase.__qualified) · [`__parameters`](#Std.Reflection.MethodBase.__parameters) · [`GetParameters`](#Std.Reflection.MethodBase.GetParameters())

<a id="Std.Reflection.MethodBase.IsStatic"></a>
### bool IsStatic

[`src/libraries/z42.core/src/Reflection/MethodBase.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodBase.z42#L10)

是否为静态方法（无隐式 `this`）。构造器永不静态。

<a id="Std.Reflection.MethodBase.__qualified"></a>
### string __qualified

[`src/libraries/z42.core/src/Reflection/MethodBase.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodBase.z42#L13)

限定后备函数名（VM 写入），用于反射调用及（`MethodInfo` 上）attribute 工厂解析。

<a id="Std.Reflection.MethodBase.__parameters"></a>
### ParameterInfo[] __parameters

[`src/libraries/z42.core/src/Reflection/MethodBase.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodBase.z42#L16)

VM 写入的后备参数数组，经 `GetParameters()` 暴露。

<a id="Std.Reflection.MethodBase.GetParameters()"></a>
### ParameterInfo[] GetParameters()

[`src/libraries/z42.core/src/Reflection/MethodBase.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MethodBase.z42#L19)

参数列表，按声明序（不含隐式 `this`）。

