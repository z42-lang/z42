<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MulticastException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

## MulticastException

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:6`

`MulticastAction.Invoke(continueOnException=true)` 时聚合所有 handler 抛出的异常。

`Failures` 与 `FailureIndices` 是并行数组；索引统一 0-based。

```z42
public class MulticastException : AggregateException {
    Exception[] Failures
    int[] FailureIndices
    int TotalHandlers
    MulticastException(Exception[] failures, int[] indices, int totalHandlers)
    int SuccessCount()
}
```

### Exception[] Failures

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:8`

各 handler 抛出的异常。

### int[] FailureIndices

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:10`

与 Failures 并行的失败下标。

### int TotalHandlers

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:12`

参与调用的 handler 总数。

### MulticastException(Exception[] failures, int[] indices, int totalHandlers)

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:15`

用失败异常、失败下标与 handler 总数构造。

### int SuccessCount()

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:23`

成功的 handler 数（= TotalHandlers - Failures.Length）。

## MulticastException<TResult>

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:34`

泛型版本，额外携带 `Results: TResult[]`：与 handler 结果等长，失败位置占位 `default(TResult)`。

```z42
public class MulticastException<TResult> : MulticastException {
    TResult[] Results
    MulticastException(Exception[] failures, int[] indices, int totalHandlers, TResult[] results)
}
```

### TResult[] Results

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:36`

各 handler 的结果（失败位置占位 `default(TResult)`）。

### MulticastException(Exception[] failures, int[] indices, int totalHandlers, TResult[] results)

`src/libraries/z42.core/src/Exceptions/MulticastException.z42:39`

用失败异常、失败下标、handler 总数与结果数组构造。

