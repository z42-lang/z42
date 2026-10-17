<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ConstructorInfo

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/ConstructorInfo.z42:7`

反射构造器，对标 C# `System.Reflection.ConstructorInfo`，由 `Type.GetConstructors()` 枚举。

与 `MethodInfo.Invoke`（在既有接收者上调方法）不同，`Invoke` 分配新实例并跑构造体，返回构造
出的对象（带参构造）。构造器永不静态，继承自 MethodBase 的 `IsStatic` 恒为 false。

```z42
public class ConstructorInfo : MethodBase {
    extern object Invoke(object[] args)
}
```

### extern object Invoke(object[] args)

`src/libraries/z42.core/src/Reflection/ConstructorInfo.z42:11`

分配新实例，用 `args`（按声明序映射形参）跑构造体并返回构造出的对象。实参个数不符抛 `Std.Exception`；构造体内的 `throw` 以原类型传播。

