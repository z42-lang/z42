<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ModuleLoader

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L11)

把已编译的 zpkg / zbc 并入**当前 VM**，以及按全限定名调用 / 实例化其中的成员。

函数与类型变得可调用、可反射。与 `AssemblyLoadContext.Load`（反射投影、只读
枚举）的区别：本类加载的代码并入 root 上下文，**可执行**。

```z42
public static class ModuleLoader {
    static void Load(string path)
    static extern bool LoadBytes(byte[] bytes)
    static extern object Invoke(string fqn)
    static object Create(string typeFqn)
    static extern string[] RunIsolated(string[] paths, string[] entries, string libsDir, int jobs)
}
```

成员：[`Load`](#Std.Runtime.ModuleLoader.Load(string)) · [`LoadBytes`](#Std.Runtime.ModuleLoader.LoadBytes(byte[])) · [`Invoke`](#Std.Runtime.ModuleLoader.Invoke(string)) · [`Create`](#Std.Runtime.ModuleLoader.Create(string)) · [`RunIsolated`](#Std.Runtime.ModuleLoader.RunIsolated(string[],string[],string,int))

<a id="Std.Runtime.ModuleLoader.Load(string)"></a>
### static void Load(string path)

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L14)

按路径加载 .zpkg / .zbc（含依赖闭包）。加载失败（文件缺失 / 格式代差 / indexed zpkg 缺散装 zbc）抛异常。

<a id="Std.Runtime.ModuleLoader.LoadBytes(byte[])"></a>
### static extern bool LoadBytes(byte[] bytes)

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L20)

从内存字节加载（packed zpkg 或裸 zbc，按 magic 区分），零落盘。成功返回 true；畸形字节抛异常。

<a id="Std.Runtime.ModuleLoader.Invoke(string)"></a>
### static extern object Invoke(string fqn)

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L25)

按全限定名调用一个**零参**静态 / 自由函数，返回其（装箱的）结果（void → null）。 被调函数里抛出的异常保持原类型向外传播。

<a id="Std.Runtime.ModuleLoader.Create(string)"></a>
### static object Create(string typeFqn)

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L31)

按全限定类型名实例化（无参构造）。

类型解析不到返回 null —— 调用方据此给出带上下文的诊断，再用 `as <接口>` 做
契约校验（接口对不上同样得到 null）。

<a id="Std.Runtime.ModuleLoader.RunIsolated(string[],string[],string,int)"></a>
### static extern string[] RunIsolated(string[] paths, string[] entries, string libsDir, int jobs)

[`src/libraries/z42.core/src/Runtime/ModuleLoader.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Runtime/ModuleLoader.z42#L43)

把每个程序放进一个**全新隔离的 VM 上下文**运行，按输入顺序返回各自捕获的 stdout。

程序为自包含的 app.zpkg / .zbc，各自拥有独立的堆、静态字段、函数表。
`entries[i]` 是 `paths[i]` 的入口全限定名（"" = 产物内置入口）；`libsDir`
提供标准库；`jobs` 个上下文并行（<=0 = 按核数，1 = 串行）。

