<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Runtime

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

`src/libraries/z42.core/src/Runtime.z42:13`

VM 运行期入口：按逻辑名动态加载 zpkg，并按全限定名静态派发固定签名入口点。

```z42
public static class Runtime {
    static extern void LoadZpkg(string name)
    static extern int CallStatic(string fqn, string[] args)
}
```

### static extern void LoadZpkg(string name)

`src/libraries/z42.core/src/Runtime.z42:20`

按逻辑名把 z42 包加载进 VM 的类型与函数注册表。幂等：同名重复加载是 no-op。 名称无法解析、文件非合法 zpkg 或 zbc 版本与运行 VM 不兼容时抛 RuntimeException。

### static extern int CallStatic(string fqn, string[] args)

`src/libraries/z42.core/src/Runtime.z42:25`

按全限定名调用 public static 函数，目标必须是精确签名 `(string[])->int`； 不匹配抛 RuntimeException。典型用于 LoadZpkg 后驱动 workload 入口并返回退出码。

