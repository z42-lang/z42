<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SkipSignal

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Failure.z42:47`

运行期跳过的信号：`Assert.Skip(reason)` 抛。runner 据其标记 Skipped。

```z42
public class SkipSignal : Exception {
    string Reason
    SkipSignal(string reason)
}
```

### string Reason

`src/libraries/z42.core/src/Failure.z42:49`

跳过原因（必须非空字符串）。

### SkipSignal(string reason)

`src/libraries/z42.core/src/Failure.z42:52`

以跳过原因构造。

