<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TestFailure

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Failure.z42:15`

断言失败的异常：`Std.Assert.*` 失败时抛。runner 据其区分「故意失败」与意外错误。

```z42
public class TestFailure : Exception {
    string Actual
    string Expected
    string Location
    TestFailure(string message)
    TestFailure(string message, string actual, string expected)
}
```

### string Actual

`src/libraries/z42.core/src/Failure.z42:17`

失败的实际值（字符串化）。空字符串表示无值（如 `Assert.Fail("...")`）。

### string Expected

`src/libraries/z42.core/src/Failure.z42:19`

期望的值（字符串化）。空字符串表示纯断言（如 `Assert.True` 失败）。

### string Location

`src/libraries/z42.core/src/Failure.z42:21`

失败位置（file:line），由 Assert 内部填入；当前留空。

### TestFailure(string message)

`src/libraries/z42.core/src/Failure.z42:24`

仅 message 的构造（适用于显式 `Assert.Fail`）。

### TestFailure(string message, string actual, string expected)

`src/libraries/z42.core/src/Failure.z42:31`

带 actual/expected 的构造（适用于比较类断言）。

