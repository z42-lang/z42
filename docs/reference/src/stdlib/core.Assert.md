<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Assert

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Assert.z42:11`

全仓唯一的断言 API。失败抛 `TestFailure`，运行期跳过抛 `SkipSignal`。

```z42
public static class Assert {
    static void Equal(object expected, object actual)
    static void NotEqual(object expected, object actual)
    static void True(bool condition)
    static void False(bool condition)
    static void Null(object? value)
    static void NotNull(object? value)
    static void Contains(string expected, string actual)
    static void Fail(string message)
    static void Skip(string reason)
    static void Throws(string expectedTypeName, Action body)
    static void ThrowsAny(Action body)
    static void DoesNotThrow(Action body)
    static void EqualApprox(double actual, double expected, double eps)
    static void Greater(long actual, long expected)
    static void Greater(double actual, double expected)
    static void Less(long actual, long expected)
    static void Less(double actual, double expected)
    static void GreaterOrEqual(long actual, long expected)
    static void GreaterOrEqual(double actual, double expected)
    static void LessOrEqual(long actual, long expected)
    static void LessOrEqual(double actual, double expected)
    static void InRange(long actual, long min, long max)
    static void InRange(double actual, double min, double max)
    static void ArrayContains(object needle, object[] haystack)
    static void ArrayDoesNotContain(object needle, object[] haystack)
    static void ArrayIsEmpty(object[] coll)
    static void ArrayIsNotEmpty(object[] coll)
}
```

### static void Equal(object expected, object actual)

`src/libraries/z42.core/src/Assert.z42:14`

断言 `expected` 与 `actual` 值相等（按 `Equals`）。

### static void NotEqual(object expected, object actual)

`src/libraries/z42.core/src/Assert.z42:24`

断言两者不相等。

### static void True(bool condition)

`src/libraries/z42.core/src/Assert.z42:33`

断言 `condition` 为 true。

### static void False(bool condition)

`src/libraries/z42.core/src/Assert.z42:40`

断言 `condition` 为 false。

### static void Null(object? value)

`src/libraries/z42.core/src/Assert.z42:47`

断言 `value` 为 null。

### static void NotNull(object? value)

`src/libraries/z42.core/src/Assert.z42:57`

断言 `value` 非 null。

### static void Contains(string expected, string actual)

`src/libraries/z42.core/src/Assert.z42:65`

断言 `actual` 包含子串 `expected`。

### static void Fail(string message)

`src/libraries/z42.core/src/Assert.z42:76`

显式失败（用于测试方法侧不可达分支等）。

### static void Skip(string reason)

`src/libraries/z42.core/src/Assert.z42:81`

运行期跳过（与 `[Skip(...)]` 编译时跳过互补：用于运行期才能判定的跳过条件）。

### static void Throws(string expectedTypeName, Action body)

`src/libraries/z42.core/src/Assert.z42:89`

断言 `body` 抛出运行时类型短名等于 `expectedTypeName` 的异常。 整测试级别的抛异常断言（类型在 attribute 期静态可知）用 `[ShouldThrow<E>]`。

### static void ThrowsAny(Action body)

`src/libraries/z42.core/src/Assert.z42:109`

断言 `body` 抛出任意异常。类型安全性低于带类型名的重载，只在只关心「有没有抛」时用。

### static void DoesNotThrow(Action body)

`src/libraries/z42.core/src/Assert.z42:119`

断言 `body` 不抛异常。

### static void EqualApprox(double actual, double expected, double eps)

`src/libraries/z42.core/src/Assert.z42:134`

绝对容差比较：`|actual - expected| <= eps`。NaN 永不命中（NaN != NaN）。

### static void Greater(long actual, long expected)

`src/libraries/z42.core/src/Assert.z42:155`

断言 `actual > expected`。

### static void Greater(double actual, double expected)

`src/libraries/z42.core/src/Assert.z42:165`

断言 `actual > expected`（NaN 操作数直接判失败）。

### static void Less(long actual, long expected)

`src/libraries/z42.core/src/Assert.z42:181`

断言 `actual < expected`。

### static void Less(double actual, double expected)

`src/libraries/z42.core/src/Assert.z42:191`

断言 `actual < expected`（NaN 操作数直接判失败）。

### static void GreaterOrEqual(long actual, long expected)

`src/libraries/z42.core/src/Assert.z42:207`

断言 `actual >= expected`。

### static void GreaterOrEqual(double actual, double expected)

`src/libraries/z42.core/src/Assert.z42:217`

断言 `actual >= expected`（NaN 操作数直接判失败）。

### static void LessOrEqual(long actual, long expected)

`src/libraries/z42.core/src/Assert.z42:233`

断言 `actual <= expected`。

### static void LessOrEqual(double actual, double expected)

`src/libraries/z42.core/src/Assert.z42:243`

断言 `actual <= expected`（NaN 操作数直接判失败）。

### static void InRange(long actual, long min, long max)

`src/libraries/z42.core/src/Assert.z42:259`

断言 `min <= actual <= max`（含边界）。

### static void InRange(double actual, double min, double max)

`src/libraries/z42.core/src/Assert.z42:269`

断言 `min <= actual <= max`（含边界；NaN 操作数直接判失败）。

### static void ArrayContains(object needle, object[] haystack)

`src/libraries/z42.core/src/Assert.z42:294`

断言 `haystack` 含 `needle`（用 `==` 比较）。

### static void ArrayDoesNotContain(object needle, object[] haystack)

`src/libraries/z42.core/src/Assert.z42:307`

断言 `haystack` 不含 `needle`。

### static void ArrayIsEmpty(object[] coll)

`src/libraries/z42.core/src/Assert.z42:319`

断言 `coll` 为空数组。

### static void ArrayIsNotEmpty(object[] coll)

`src/libraries/z42.core/src/Assert.z42:329`

断言 `coll` 非空数组。

