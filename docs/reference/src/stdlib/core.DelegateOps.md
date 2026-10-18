<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DelegateOps

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Delegates/DelegateOps.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/DelegateOps.z42#L10)

委托引用相等与拆解工具。

运行时 Action<T> / Func<T,R> / Predicate<T> 等委托是 Z42FuncType 值（VM 的 Value::FuncRef /
Value::Closure 二者之一）。ReferenceEquals 按各自身份比较：FuncRef 看目标函数相同；
Closure 看目标函数相同 + env 的 GC ptr_eq；跨变体不等；非委托值返回 false 不报错。

```z42
public static class DelegateOps {
    static extern bool ReferenceEquals(object a, object b)
    static extern object GetTarget(object delegateValue)
    static extern string GetFnName(object delegateValue)
    static extern object MakeClosure(string fnName, object[] env)
}
```

成员：[`ReferenceEquals`](#Std.DelegateOps.ReferenceEquals(object,object)) · [`GetTarget`](#Std.DelegateOps.GetTarget(object)) · [`GetFnName`](#Std.DelegateOps.GetFnName(object)) · [`MakeClosure`](#Std.DelegateOps.MakeClosure(string,object[]))

<a id="Std.DelegateOps.ReferenceEquals(object,object)"></a>
### static extern bool ReferenceEquals(object a, object b)

[`src/libraries/z42.core/src/Delegates/DelegateOps.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/DelegateOps.z42#L13)

判断两个委托值是否引用相等。

<a id="Std.DelegateOps.GetTarget(object)"></a>
### static extern object GetTarget(object delegateValue)

[`src/libraries/z42.core/src/Delegates/DelegateOps.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/DelegateOps.z42#L17)

从委托提取捕获的目标对象（receiver）；无可提取目标返回 null。

<a id="Std.DelegateOps.GetFnName(object)"></a>
### static extern string GetFnName(object delegateValue)

[`src/libraries/z42.core/src/Delegates/DelegateOps.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/DelegateOps.z42#L21)

提取委托的 thunk 函数全限定名。

<a id="Std.DelegateOps.MakeClosure(string,object[])"></a>
### static extern object MakeClosure(string fnName, object[] env)

[`src/libraries/z42.core/src/Delegates/DelegateOps.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/DelegateOps.z42#L25)

用函数名 + 环境数组重建闭包；env[0] 通常是 receiver 对象。

