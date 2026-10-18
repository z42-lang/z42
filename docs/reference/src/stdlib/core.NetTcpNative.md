<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.NetTcpNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:166`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L166)

种子兼容（旧形状）：TCP socket 原语，返回判别数组。

```z42
public static class NetTcpNative {
    static extern object[] Connect(string host, int port)
    static extern object[] Listen(string host, int port)
    static extern object[] Accept(long listenerSlotId)
    static extern object[] SocketRead(long slotId, byte[] buffer, int offset, int count)
    static extern object[] SocketWrite(long slotId, byte[] buffer, int offset, int count)
    static extern void SocketDrop(long slotId)
    static extern void ListenerDrop(long slotId)
    static extern object[] SocketSetReadTimeout(long slotId, int millis)
    static extern object[] SocketSetWriteTimeout(long slotId, int millis)
    static extern object[] SocketSetNoDelay(long slotId, bool enable)
    static extern object[] SocketSetTtl(long slotId, int ttl)
    static extern object[] ListenerSetTtl(long slotId, int ttl)
    static extern object[] ConnectWithTimeout(string host, int port, int millis)
    static extern object[] SocketSetKeepalive(long slotId, bool enable)
    static extern object[] SocketSetKeepaliveTuned(long slotId, bool enable, long idleSecs, long intervalSecs, long probes)
    static extern object[] ListenWithOptions(string host, int port, bool reuseAddr)
}
```

成员：[`Connect`](#Std.Net.Sockets.NetTcpNative.Connect(string,int)) · [`Listen`](#Std.Net.Sockets.NetTcpNative.Listen(string,int)) · [`Accept`](#Std.Net.Sockets.NetTcpNative.Accept(long)) · [`SocketRead`](#Std.Net.Sockets.NetTcpNative.SocketRead(long,byte[],int,int)) · [`SocketWrite`](#Std.Net.Sockets.NetTcpNative.SocketWrite(long,byte[],int,int)) · [`SocketDrop`](#Std.Net.Sockets.NetTcpNative.SocketDrop(long)) · [`ListenerDrop`](#Std.Net.Sockets.NetTcpNative.ListenerDrop(long)) · [`SocketSetReadTimeout`](#Std.Net.Sockets.NetTcpNative.SocketSetReadTimeout(long,int)) · [`SocketSetWriteTimeout`](#Std.Net.Sockets.NetTcpNative.SocketSetWriteTimeout(long,int)) · [`SocketSetNoDelay`](#Std.Net.Sockets.NetTcpNative.SocketSetNoDelay(long,bool)) · [`SocketSetTtl`](#Std.Net.Sockets.NetTcpNative.SocketSetTtl(long,int)) · [`ListenerSetTtl`](#Std.Net.Sockets.NetTcpNative.ListenerSetTtl(long,int)) · [`ConnectWithTimeout`](#Std.Net.Sockets.NetTcpNative.ConnectWithTimeout(string,int,int)) · [`SocketSetKeepalive`](#Std.Net.Sockets.NetTcpNative.SocketSetKeepalive(long,bool)) · [`SocketSetKeepaliveTuned`](#Std.Net.Sockets.NetTcpNative.SocketSetKeepaliveTuned(long,bool,long,long,long)) · [`ListenWithOptions`](#Std.Net.Sockets.NetTcpNative.ListenWithOptions(string,int,bool))

<a id="Std.Net.Sockets.NetTcpNative.Connect(string,int)"></a>
### static extern object[] Connect(string host, int port)

[`src/libraries/z42.core/src/Native/NetNative.z42:169`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L169)

（种子兼容）连接。

<a id="Std.Net.Sockets.NetTcpNative.Listen(string,int)"></a>
### static extern object[] Listen(string host, int port)

[`src/libraries/z42.core/src/Native/NetNative.z42:173`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L173)

（种子兼容）监听。

<a id="Std.Net.Sockets.NetTcpNative.Accept(long)"></a>
### static extern object[] Accept(long listenerSlotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L177)

（种子兼容）接受连接。

<a id="Std.Net.Sockets.NetTcpNative.SocketRead(long,byte[],int,int)"></a>
### static extern object[] SocketRead(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:181`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L181)

（种子兼容）读。

<a id="Std.Net.Sockets.NetTcpNative.SocketWrite(long,byte[],int,int)"></a>
### static extern object[] SocketWrite(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:185`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L185)

（种子兼容）写。

<a id="Std.Net.Sockets.NetTcpNative.SocketDrop(long)"></a>
### static extern void SocketDrop(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:189`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L189)

（种子兼容）关闭 socket。

<a id="Std.Net.Sockets.NetTcpNative.ListenerDrop(long)"></a>
### static extern void ListenerDrop(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:193`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L193)

（种子兼容）关闭监听 socket。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetReadTimeout(long,int)"></a>
### static extern object[] SocketSetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:197`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L197)

（种子兼容）设置读超时；millis <= 0 清除超时。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetWriteTimeout(long,int)"></a>
### static extern object[] SocketSetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:201`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L201)

（种子兼容）设置写超时；millis <= 0 清除超时。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetNoDelay(long,bool)"></a>
### static extern object[] SocketSetNoDelay(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:205`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L205)

（种子兼容）设置 TCP_NODELAY。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetTtl(long,int)"></a>
### static extern object[] SocketSetTtl(long slotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:209`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L209)

（种子兼容）设置 IP_TTL。

<a id="Std.Net.Sockets.NetTcpNative.ListenerSetTtl(long,int)"></a>
### static extern object[] ListenerSetTtl(long slotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:213`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L213)

（种子兼容）设置监听 IP_TTL。

<a id="Std.Net.Sockets.NetTcpNative.ConnectWithTimeout(string,int,int)"></a>
### static extern object[] ConnectWithTimeout(string host, int port, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:217`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L217)

（种子兼容）带超时连接。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetKeepalive(long,bool)"></a>
### static extern object[] SocketSetKeepalive(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L221)

（种子兼容）设置 SO_KEEPALIVE。

<a id="Std.Net.Sockets.NetTcpNative.SocketSetKeepaliveTuned(long,bool,long,long,long)"></a>
### static extern object[] SocketSetKeepaliveTuned(long slotId, bool enable, long idleSecs, long intervalSecs, long probes)

[`src/libraries/z42.core/src/Native/NetNative.z42:225`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L225)

（种子兼容）带调参的 SO_KEEPALIVE。

<a id="Std.Net.Sockets.NetTcpNative.ListenWithOptions(string,int,bool)"></a>
### static extern object[] ListenWithOptions(string host, int port, bool reuseAddr)

[`src/libraries/z42.core/src/Native/NetNative.z42:231`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L231)

（种子兼容）带选项监听。

