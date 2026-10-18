<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SplitOptions

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/SplitOptions.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/SplitOptions.z42#L8)

`String.Split(separator, options)` 的按位选项标志。

```z42
public static class SplitOptions {
    static int None
    static int RemoveEmptyEntries
    static int TrimEntries
}
```

成员：[`None`](#Std.SplitOptions.None) · [`RemoveEmptyEntries`](#Std.SplitOptions.RemoveEmptyEntries) · [`TrimEntries`](#Std.SplitOptions.TrimEntries)

<a id="Std.SplitOptions.None"></a>
### static int None

[`src/libraries/z42.core/src/SplitOptions.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/SplitOptions.z42#L10)

不后处理（等价单参 `Split(separator)`）。

<a id="Std.SplitOptions.RemoveEmptyEntries"></a>
### static int RemoveEmptyEntries

[`src/libraries/z42.core/src/SplitOptions.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/SplitOptions.z42#L13)

丢弃长度为 0 的段。适合分隔符相邻/首尾会导致空段的分行、分字段场景。

<a id="Std.SplitOptions.TrimEntries"></a>
### static int TrimEntries

[`src/libraries/z42.core/src/SplitOptions.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/SplitOptions.z42#L17)

对每段做 `Trim()`（去首尾空白）。可与 `RemoveEmptyEntries` 组合，一并丢弃 trim 后变空的段。

