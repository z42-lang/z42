<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DnsNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:155`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L155)

DNS 解析的 native 原语，供 z42.net 的 Dns 调用。

```z42
public static class DnsNative {
    static extern string[] Resolve(string host)
    static extern object[] Lookup(string host)
}
```

成员：[`Resolve`](#Std.Net.Sockets.DnsNative.Resolve(string)) · [`Lookup`](#Std.Net.Sockets.DnsNative.Lookup(string))

<a id="Std.Net.Sockets.DnsNative.Resolve(string)"></a>
### static extern string[] Resolve(string host)

[`src/libraries/z42.core/src/Native/NetNative.z42:158`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L158)

解析出的地址（文本形式，去重）；解析失败抛 `SocketException(HostNotFound)`。

<a id="Std.Net.Sockets.DnsNative.Lookup(string)"></a>
### static extern object[] Lookup(string host)

[`src/libraries/z42.core/src/Native/NetNative.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L162)

（种子兼容，旧形状）返回 `[0, string[]]` ok / `[1, msg]` err / `[3]` unsupported。

