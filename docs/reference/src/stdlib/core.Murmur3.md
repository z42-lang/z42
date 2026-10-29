<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Murmur3

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Hashing`

[`src/libraries/z42.core/src/Hashing/Murmur3.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Hashing/Murmur3.z42#L12)

MurmurHash3 x86_128：快速的**非加密**哈希，用于内容标识与变更检测（构建产物 id、缓存键）。

不抗碰撞攻击，不能用于任何安全边界——需要密码学强度时用 `Std.Crypto` 的 `Sha256` / `Blake3`。
输出 16 字节：h1..h4 各按小端写出，与参考实现（smhasher 的 `MurmurHash3_x86_128`）一致；seed 固定 0。

```z42
public static class Murmur3 {
    static byte[] Hash128(byte[] data)
    static extern string Hash128Hex(byte[] data)
    static extern string Hash128Hex(string text)
}
```

成员：[`Hash128`](#Std.Hashing.Murmur3.Hash128(byte[])) · [`Hash128Hex`](#Std.Hashing.Murmur3.Hash128Hex(byte[])) · [`Hash128Hex`](#Std.Hashing.Murmur3.Hash128Hex(string))

<a id="Std.Hashing.Murmur3.Hash128(byte[])"></a>
### static byte[] Hash128(byte[] data)

[`src/libraries/z42.core/src/Hashing/Murmur3.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Hashing/Murmur3.z42#L32)

128 位摘要（16 字节，小端 h1..h4）。

<a id="Std.Hashing.Murmur3.Hash128Hex(byte[])"></a>
### static extern string Hash128Hex(byte[] data)

[`src/libraries/z42.core/src/Hashing/Murmur3.z42:120`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Hashing/Murmur3.z42#L120)

`Hash128(data)` 的 32 个小写十六进制字符（无前缀）。

<a id="Std.Hashing.Murmur3.Hash128Hex(string)"></a>
### static extern string Hash128Hex(string text)

[`src/libraries/z42.core/src/Hashing/Murmur3.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Hashing/Murmur3.z42#L125)

`text` 按 UTF-8 编码后的 `Hash128` 的 32 个小写十六进制字符，等价于 `Hash128Hex(Utf8.GetBytes(text))`，但不物化中间字节数组。

