<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MemberInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/MemberInfo.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MemberInfo.z42#L7)

所有反射成员（字段、方法）的基类。

按约定只读——实例只由 VM 反射内建（`__type_fields` / `__type_methods`）创建并
填充，用户代码不构造。

```z42
public class MemberInfo {
    string Name
}
```

成员：[`Name`](#Std.Reflection.MemberInfo.Name)

<a id="Std.Reflection.MemberInfo.Name"></a>
### string Name

[`src/libraries/z42.core/src/Reflection/MemberInfo.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/MemberInfo.z42#L9)

成员名（非限定）。VM 直接写入。

