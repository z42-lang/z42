<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Assembly

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Reflection`

`src/libraries/z42.core/src/Reflection/Assembly.z42:8`

一个 zpkg 的运行时投影，对标 .NET `System.Reflection.Assembly`。

载入 collectible 上下文的 zpkg 身份被保留为 Assembly，可反射其类型、查所属
`AssemblyLoadContext`、判其是否可回收。不可用户构造——由 `AssemblyLoadContext.Load` /
`Type.Assembly` / `AssemblyLoadContext.GetAssemblies` 产出。

```z42
public sealed class Assembly {
    extern string Name { get; }
    extern bool IsCollectible { get; }
    extern Std.Runtime.AssemblyLoadContext LoadContext { get; }
    extern Std.Type[] GetTypes()
    extern Std.Reflection.MethodInfo[] GetFunctions()
}
```

### extern string Name { get; }

`src/libraries/z42.core/src/Reflection/Assembly.z42:11`

zpkg 的逻辑名。

### extern bool IsCollectible { get; }

`src/libraries/z42.core/src/Reflection/Assembly.z42:15`

是否在可回收上下文（等于其 AssemblyLoadContext.IsCollectible）。

### extern Std.Runtime.AssemblyLoadContext LoadContext { get; }

`src/libraries/z42.core/src/Reflection/Assembly.z42:19`

所属加载上下文。

### extern Std.Type[] GetTypes()

`src/libraries/z42.core/src/Reflection/Assembly.z42:23`

本 assembly 定义的类型，按全限定名有序。

### extern Std.Reflection.MethodInfo[] GetFunctions()

`src/libraries/z42.core/src/Reflection/Assembly.z42:30`

本 assembly 定义的自由（顶层）函数，按全限定名有序；编译器合成的函数（lambda、attribute 工厂等）不在内。

返回的 MethodInfo 按全限定名在当前 VM 解析：要读 attribute、签名或 Invoke，该包须已
`Std.Runtime.ModuleLoader.Load` 并入；纯反射投影只保证名字可枚举。

