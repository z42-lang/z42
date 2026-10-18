<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SkipSignal

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Failure.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Failure.z42#L47)

运行期跳过的信号：`Assert.Skip(reason)` 抛。runner 据其标记 Skipped。

```z42
public class SkipSignal : Exception {
    string Reason
    SkipSignal(string reason)
}
```

成员：[`Reason`](#Std.SkipSignal.Reason) · [`SkipSignal`](#Std.SkipSignal.ctor(string))

<a id="Std.SkipSignal.Reason"></a>
### string Reason

[`src/libraries/z42.core/src/Failure.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Failure.z42#L49)

跳过原因（必须非空字符串）。

<a id="Std.SkipSignal.ctor(string)"></a>
### SkipSignal(string reason)

[`src/libraries/z42.core/src/Failure.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Failure.z42#L52)

以跳过原因构造。

