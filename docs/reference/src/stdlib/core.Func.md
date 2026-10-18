<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Func

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

## Func<R>

[`src/libraries/z42.core/src/Delegates/Delegates.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/Delegates.z42#L17)

无参、返回 R。

```z42
public delegate R Func<R>()
```

## Func<T,R>

[`src/libraries/z42.core/src/Delegates/Delegates.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/Delegates.z42#L19)

单参数、返回 R。

```z42
public delegate R Func<T,R>(T arg)
```

## Func<T1,T2,R>

[`src/libraries/z42.core/src/Delegates/Delegates.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/Delegates.z42#L21)

双参数、返回 R。

```z42
public delegate R Func<T1,T2,R>(T1 a, T2 b)
```

## Func<T1,T2,T3,R>

[`src/libraries/z42.core/src/Delegates/Delegates.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/Delegates.z42#L23)

三参数、返回 R。

```z42
public delegate R Func<T1,T2,T3,R>(T1 a, T2 b, T3 c)
```

## Func<T1,T2,T3,T4,R>

[`src/libraries/z42.core/src/Delegates/Delegates.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Delegates/Delegates.z42#L25)

四参数、返回 R。

```z42
public delegate R Func<T1,T2,T3,T4,R>(T1 a, T2 b, T3 c, T4 d)
```

