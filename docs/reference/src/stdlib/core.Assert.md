<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Assert

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Assert.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L11)

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

成员：[`Equal`](#Std.Assert.Equal(object,object)) · [`NotEqual`](#Std.Assert.NotEqual(object,object)) · [`True`](#Std.Assert.True(bool)) · [`False`](#Std.Assert.False(bool)) · [`Null`](#Std.Assert.Null(object?)) · [`NotNull`](#Std.Assert.NotNull(object?)) · [`Contains`](#Std.Assert.Contains(string,string)) · [`Fail`](#Std.Assert.Fail(string)) · [`Skip`](#Std.Assert.Skip(string)) · [`Throws`](#Std.Assert.Throws(string,Action)) · [`ThrowsAny`](#Std.Assert.ThrowsAny(Action)) · [`DoesNotThrow`](#Std.Assert.DoesNotThrow(Action)) · [`EqualApprox`](#Std.Assert.EqualApprox(double,double,double)) · [`Greater`](#Std.Assert.Greater(long,long)) · [`Greater`](#Std.Assert.Greater(double,double)) · [`Less`](#Std.Assert.Less(long,long)) · [`Less`](#Std.Assert.Less(double,double)) · [`GreaterOrEqual`](#Std.Assert.GreaterOrEqual(long,long)) · [`GreaterOrEqual`](#Std.Assert.GreaterOrEqual(double,double)) · [`LessOrEqual`](#Std.Assert.LessOrEqual(long,long)) · [`LessOrEqual`](#Std.Assert.LessOrEqual(double,double)) · [`InRange`](#Std.Assert.InRange(long,long,long)) · [`InRange`](#Std.Assert.InRange(double,double,double)) · [`ArrayContains`](#Std.Assert.ArrayContains(object,object[])) · [`ArrayDoesNotContain`](#Std.Assert.ArrayDoesNotContain(object,object[])) · [`ArrayIsEmpty`](#Std.Assert.ArrayIsEmpty(object[])) · [`ArrayIsNotEmpty`](#Std.Assert.ArrayIsNotEmpty(object[]))

<a id="Std.Assert.Equal(object,object)"></a>
### static void Equal(object expected, object actual)

[`src/libraries/z42.core/src/Assert.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L14)

断言 `expected` 与 `actual` 值相等（按 `Equals`）。

<a id="Std.Assert.NotEqual(object,object)"></a>
### static void NotEqual(object expected, object actual)

[`src/libraries/z42.core/src/Assert.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L24)

断言两者不相等。

<a id="Std.Assert.True(bool)"></a>
### static void True(bool condition)

[`src/libraries/z42.core/src/Assert.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L33)

断言 `condition` 为 true。

<a id="Std.Assert.False(bool)"></a>
### static void False(bool condition)

[`src/libraries/z42.core/src/Assert.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L40)

断言 `condition` 为 false。

<a id="Std.Assert.Null(object?)"></a>
### static void Null(object? value)

[`src/libraries/z42.core/src/Assert.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L47)

断言 `value` 为 null。

<a id="Std.Assert.NotNull(object?)"></a>
### static void NotNull(object? value)

[`src/libraries/z42.core/src/Assert.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L57)

断言 `value` 非 null。

<a id="Std.Assert.Contains(string,string)"></a>
### static void Contains(string expected, string actual)

[`src/libraries/z42.core/src/Assert.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L65)

断言 `actual` 包含子串 `expected`。

<a id="Std.Assert.Fail(string)"></a>
### static void Fail(string message)

[`src/libraries/z42.core/src/Assert.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L76)

显式失败（用于测试方法侧不可达分支等）。

<a id="Std.Assert.Skip(string)"></a>
### static void Skip(string reason)

[`src/libraries/z42.core/src/Assert.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L81)

运行期跳过（与 `[Skip(...)]` 编译时跳过互补：用于运行期才能判定的跳过条件）。

<a id="Std.Assert.Throws(string,Action)"></a>
### static void Throws(string expectedTypeName, Action body)

[`src/libraries/z42.core/src/Assert.z42:91`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L91)

断言 `body` 抛出类型短名等于 `expectedTypeName` 的异常。

匹配的是运行时类型短名。整测试级别的抛异常断言（类型在 attribute 期静态
可知）用 `[ShouldThrow<E>]`。

<a id="Std.Assert.ThrowsAny(Action)"></a>
### static void ThrowsAny(Action body)

[`src/libraries/z42.core/src/Assert.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L111)

断言 `body` 抛出任意异常。类型安全性低于带类型名的重载，只在只关心「有没有抛」时用。

<a id="Std.Assert.DoesNotThrow(Action)"></a>
### static void DoesNotThrow(Action body)

[`src/libraries/z42.core/src/Assert.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L121)

断言 `body` 不抛异常。

<a id="Std.Assert.EqualApprox(double,double,double)"></a>
### static void EqualApprox(double actual, double expected, double eps)

[`src/libraries/z42.core/src/Assert.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L136)

绝对容差比较：`|actual - expected| <= eps`。NaN 永不命中（NaN != NaN）。

<a id="Std.Assert.Greater(long,long)"></a>
### static void Greater(long actual, long expected)

[`src/libraries/z42.core/src/Assert.z42:157`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L157)

断言 `actual > expected`。

<a id="Std.Assert.Greater(double,double)"></a>
### static void Greater(double actual, double expected)

[`src/libraries/z42.core/src/Assert.z42:167`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L167)

断言 `actual > expected`（NaN 操作数直接判失败）。

<a id="Std.Assert.Less(long,long)"></a>
### static void Less(long actual, long expected)

[`src/libraries/z42.core/src/Assert.z42:183`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L183)

断言 `actual < expected`。

<a id="Std.Assert.Less(double,double)"></a>
### static void Less(double actual, double expected)

[`src/libraries/z42.core/src/Assert.z42:193`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L193)

断言 `actual < expected`（NaN 操作数直接判失败）。

<a id="Std.Assert.GreaterOrEqual(long,long)"></a>
### static void GreaterOrEqual(long actual, long expected)

[`src/libraries/z42.core/src/Assert.z42:209`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L209)

断言 `actual >= expected`。

<a id="Std.Assert.GreaterOrEqual(double,double)"></a>
### static void GreaterOrEqual(double actual, double expected)

[`src/libraries/z42.core/src/Assert.z42:219`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L219)

断言 `actual >= expected`（NaN 操作数直接判失败）。

<a id="Std.Assert.LessOrEqual(long,long)"></a>
### static void LessOrEqual(long actual, long expected)

[`src/libraries/z42.core/src/Assert.z42:235`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L235)

断言 `actual <= expected`。

<a id="Std.Assert.LessOrEqual(double,double)"></a>
### static void LessOrEqual(double actual, double expected)

[`src/libraries/z42.core/src/Assert.z42:245`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L245)

断言 `actual <= expected`（NaN 操作数直接判失败）。

<a id="Std.Assert.InRange(long,long,long)"></a>
### static void InRange(long actual, long min, long max)

[`src/libraries/z42.core/src/Assert.z42:261`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L261)

断言 `min <= actual <= max`（含边界）。

<a id="Std.Assert.InRange(double,double,double)"></a>
### static void InRange(double actual, double min, double max)

[`src/libraries/z42.core/src/Assert.z42:271`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L271)

断言 `min <= actual <= max`（含边界；NaN 操作数直接判失败）。

<a id="Std.Assert.ArrayContains(object,object[])"></a>
### static void ArrayContains(object needle, object[] haystack)

[`src/libraries/z42.core/src/Assert.z42:296`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L296)

断言 `haystack` 含 `needle`（用 `==` 比较）。

<a id="Std.Assert.ArrayDoesNotContain(object,object[])"></a>
### static void ArrayDoesNotContain(object needle, object[] haystack)

[`src/libraries/z42.core/src/Assert.z42:309`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L309)

断言 `haystack` 不含 `needle`。

<a id="Std.Assert.ArrayIsEmpty(object[])"></a>
### static void ArrayIsEmpty(object[] coll)

[`src/libraries/z42.core/src/Assert.z42:321`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L321)

断言 `coll` 为空数组。

<a id="Std.Assert.ArrayIsNotEmpty(object[])"></a>
### static void ArrayIsNotEmpty(object[] coll)

[`src/libraries/z42.core/src/Assert.z42:331`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Assert.z42#L331)

断言 `coll` 非空数组。

