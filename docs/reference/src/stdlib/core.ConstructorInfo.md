<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ConstructorInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/ConstructorInfo.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ConstructorInfo.z42#L7)

反射构造器，对标 C# `System.Reflection.ConstructorInfo`，由 `Type.GetConstructors()` 枚举。

与 `MethodInfo.Invoke`（在既有接收者上调方法）不同，`Invoke` 分配新实例并跑构造体，返回构造
出的对象（带参构造）。构造器永不静态，继承自 MethodBase 的 `IsStatic` 恒为 false。

```z42
public class ConstructorInfo : MethodBase {
    string __owner
    extern object Invoke(object[] args)
}
```

成员：[`__owner`](#Std.Reflection.ConstructorInfo.__owner) · [`Invoke`](#Std.Reflection.ConstructorInfo.Invoke(object[]))

<a id="Std.Reflection.ConstructorInfo.__owner"></a>
### string __owner

[`src/libraries/z42.core/src/Reflection/ConstructorInfo.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ConstructorInfo.z42#L10)

VM 写入：调用本构造器所构造的类型的规范名（从构造泛型 `Type` 枚举时带实参，如 `Demo.Box<int>`），`Invoke` 按它分配实例。

<a id="Std.Reflection.ConstructorInfo.Invoke(object[])"></a>
### extern object Invoke(object[] args)

[`src/libraries/z42.core/src/Reflection/ConstructorInfo.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/ConstructorInfo.z42#L17)

分配新实例并跑构造体，返回构造出的对象。

`args` 按声明序映射形参。实参个数不符抛 `Std.Exception`；构造体内的 `throw`
以原类型传播。

