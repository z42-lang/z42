<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.NetTlsNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:235`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L235)

种子兼容（旧形状）：TLS socket 原语，返回判别数组。

```z42
public static class NetTlsNative {
    static extern object[] Connect(string host, int port, int timeoutMs)
    static extern object[] SocketRead(long slotId, byte[] buffer, int offset, int count)
    static extern object[] SocketWrite(long slotId, byte[] buffer, int offset, int count)
    static extern void SocketDrop(long slotId)
    static extern object[] SocketSetReadTimeout(long slotId, int millis)
    static extern object[] SocketSetWriteTimeout(long slotId, int millis)
}
```

成员：[`Connect`](#Std.Net.Sockets.NetTlsNative.Connect(string,int,int)) · [`SocketRead`](#Std.Net.Sockets.NetTlsNative.SocketRead(long,byte[],int,int)) · [`SocketWrite`](#Std.Net.Sockets.NetTlsNative.SocketWrite(long,byte[],int,int)) · [`SocketDrop`](#Std.Net.Sockets.NetTlsNative.SocketDrop(long)) · [`SocketSetReadTimeout`](#Std.Net.Sockets.NetTlsNative.SocketSetReadTimeout(long,int)) · [`SocketSetWriteTimeout`](#Std.Net.Sockets.NetTlsNative.SocketSetWriteTimeout(long,int))

<a id="Std.Net.Sockets.NetTlsNative.Connect(string,int,int)"></a>
### static extern object[] Connect(string host, int port, int timeoutMs)

[`src/libraries/z42.core/src/Native/NetNative.z42:238`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L238)

（种子兼容）连接并握手；timeoutMs <= 0 → 不限时，> 0 同时约束两者。

<a id="Std.Net.Sockets.NetTlsNative.SocketRead(long,byte[],int,int)"></a>
### static extern object[] SocketRead(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:242`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L242)

（种子兼容）读。

<a id="Std.Net.Sockets.NetTlsNative.SocketWrite(long,byte[],int,int)"></a>
### static extern object[] SocketWrite(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:246`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L246)

（种子兼容）写。

<a id="Std.Net.Sockets.NetTlsNative.SocketDrop(long)"></a>
### static extern void SocketDrop(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:250`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L250)

（种子兼容）关闭。

<a id="Std.Net.Sockets.NetTlsNative.SocketSetReadTimeout(long,int)"></a>
### static extern object[] SocketSetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:254`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L254)

（种子兼容）设置读超时。

<a id="Std.Net.Sockets.NetTlsNative.SocketSetWriteTimeout(long,int)"></a>
### static extern object[] SocketSetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:258`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L258)

（种子兼容）设置写超时。

