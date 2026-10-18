<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Activator

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

[`src/libraries/z42.core/src/Reflection/Activator.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/Activator.z42#L6)

反射式构造对象（无参子集），对标 C# `System.Activator`。

带参构造 `CreateInstance(Type, args)` 未提供，改用 `ConstructorInfo.Invoke`。

```z42
public static class Activator {
    static extern object CreateInstance(Std.Type t)
    static T CreateInstance<T>()
}
```

成员：[`CreateInstance`](#Std.Reflection.Activator.CreateInstance(Std.Type)) · [`CreateInstance`](#Std.Reflection.Activator.CreateInstance%3CT%3E())

<a id="Std.Reflection.Activator.CreateInstance(Std.Type)"></a>
### static extern object CreateInstance(Std.Type t)

[`src/libraries/z42.core/src/Reflection/Activator.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/Activator.z42#L9)

通过 `t` 的无参构造器创建实例；类型无显式构造器时做默认字段分配。构造器内 `throw` 以原类型传播。

<a id="Std.Reflection.Activator.CreateInstance<T>()"></a>
### static T CreateInstance<T>()

[`src/libraries/z42.core/src/Reflection/Activator.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Reflection/Activator.z42#L12)

泛型便捷入口：按 `new T()` 无参构造，等价于 `(T)CreateInstance(typeof(T))`。

