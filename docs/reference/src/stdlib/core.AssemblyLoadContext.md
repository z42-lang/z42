<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.AssemblyLoadContext

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:6`

z42 的加载上下文（对标 dotnet `AssemblyLoadContext`）。把 zpkg 载入可界定边界的上下文： root（core / stdlib / 主程序所在，永驻不可回收）+ collectible（按需创建，可卸载）。

```z42
public sealed class AssemblyLoadContext {
    static extern AssemblyLoadContext Default()
    static extern AssemblyLoadContext CreateCollectible(string name)
    extern string Name { get; }
    extern bool IsCollectible { get; }
    extern Std.Reflection.Assembly Load(string zpkgPath)
    extern Std.Reflection.Assembly[] GetAssemblies()
    void Unload()
}
```

### static extern AssemblyLoadContext Default()

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:10`

永驻 root 上下文（core / stdlib / 主程序所在），IsCollectible == false。

### static extern AssemblyLoadContext CreateCollectible(string name)

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:14`

创建一个可回收（collectible）上下文。

### extern string Name { get; }

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:18`

上下文名字（root 恒为 "root"）。

### extern bool IsCollectible { get; }

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:22`

是否可回收。root → false；CreateCollectible 建的 → true。

### extern Std.Reflection.Assembly Load(string zpkgPath)

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:26`

把一个 zpkg 载入本上下文，返回其运行时 Assembly 投影。

### extern Std.Reflection.Assembly[] GetAssemblies()

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:30`

本上下文已载入的 assembly 列表。

### void Unload()

`src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42:34`

卸载本上下文：标记为 Unloading，由 GC 惰性回收。root 永驻不可卸载 → 抛 `InvalidOperationException`。fire-and-forget（卸载惰性，不保证立即回收）。

