<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Clock

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

[`src/libraries/z42.core/src/Clock.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Clock.z42#L7)

VM 时钟原语的唯一声明点。只在需要裸数值时直接用它；语义包装在 `Std.Time`。

```z42
public static class Clock {
    static extern long WallMillis()
    static extern long MonoNanos()
}
```

成员：[`WallMillis`](#Std.Runtime.Clock.WallMillis()) · [`MonoNanos`](#Std.Runtime.Clock.MonoNanos())

<a id="Std.Runtime.Clock.WallMillis()"></a>
### static extern long WallMillis()

[`src/libraries/z42.core/src/Clock.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Clock.z42#L10)

挂钟时间，Unix 毫秒（可能跳变；用于时间戳）。

<a id="Std.Runtime.Clock.MonoNanos()"></a>
### static extern long MonoNanos()

[`src/libraries/z42.core/src/Clock.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Clock.z42#L14)

单调高精度纳秒（不跳变；用于测时长）。

