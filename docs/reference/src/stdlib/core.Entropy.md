<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Entropy

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Runtime`

[`src/libraries/z42.core/src/Entropy.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Entropy.z42#L12)

OS 熵源 intrinsic 的唯一声明点。

封装 Linux getrandom(2) / macOS getentropy / Windows BCryptGenRandom；wasm32
抛 Std.NotSupportedException。

```z42
public static class Entropy {
    static extern byte[] GetBytes(int n)
}
```

成员：[`GetBytes`](#Std.Runtime.Entropy.GetBytes(int))

<a id="Std.Runtime.Entropy.GetBytes(int)"></a>
### static extern byte[] GetBytes(int n)

[`src/libraries/z42.core/src/Entropy.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Entropy.z42#L15)

从 OS CSPRNG 取 `n` 字节（调用方自行校验 `n`）。

