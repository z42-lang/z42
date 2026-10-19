<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TcpSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L14)

TCP socket 的 native 原语，供 z42.net 的 TcpClient / TcpListener 调用。

```z42
public static class TcpSocketNative {
    static extern long Connect(string host, int port, int timeoutMs)
    static extern long Listen(string host, int port, bool reuseAddress)
    static extern int ListenerPort(long listenerSlotId)
    static extern long Accept(long listenerSlotId)
    static extern int Read(long slotId, byte[] buffer, int offset, int count)
    static extern void Write(long slotId, byte[] buffer, int offset, int count)
    static extern void Close(long slotId)
    static extern void ListenerClose(long listenerSlotId)
    static extern void SetReadTimeout(long slotId, int millis)
    static extern void SetWriteTimeout(long slotId, int millis)
    static extern void SetNoDelay(long slotId, bool enable)
    static extern void SetTtl(long slotId, int ttl)
    static extern void ListenerSetTtl(long listenerSlotId, int ttl)
    static extern void SetKeepAlive(long slotId, bool enable)
    static extern void SetKeepAliveTuned(long slotId, bool enable, int idleSecs, int intervalSecs, int probes)
}
```

成员：[`Connect`](#Std.Net.Sockets.TcpSocketNative.Connect(string,int,int)) · [`Listen`](#Std.Net.Sockets.TcpSocketNative.Listen(string,int,bool)) · [`ListenerPort`](#Std.Net.Sockets.TcpSocketNative.ListenerPort(long)) · [`Accept`](#Std.Net.Sockets.TcpSocketNative.Accept(long)) · [`Read`](#Std.Net.Sockets.TcpSocketNative.Read(long,byte[],int,int)) · [`Write`](#Std.Net.Sockets.TcpSocketNative.Write(long,byte[],int,int)) · [`Close`](#Std.Net.Sockets.TcpSocketNative.Close(long)) · [`ListenerClose`](#Std.Net.Sockets.TcpSocketNative.ListenerClose(long)) · [`SetReadTimeout`](#Std.Net.Sockets.TcpSocketNative.SetReadTimeout(long,int)) · [`SetWriteTimeout`](#Std.Net.Sockets.TcpSocketNative.SetWriteTimeout(long,int)) · [`SetNoDelay`](#Std.Net.Sockets.TcpSocketNative.SetNoDelay(long,bool)) · [`SetTtl`](#Std.Net.Sockets.TcpSocketNative.SetTtl(long,int)) · [`ListenerSetTtl`](#Std.Net.Sockets.TcpSocketNative.ListenerSetTtl(long,int)) · [`SetKeepAlive`](#Std.Net.Sockets.TcpSocketNative.SetKeepAlive(long,bool)) · [`SetKeepAliveTuned`](#Std.Net.Sockets.TcpSocketNative.SetKeepAliveTuned(long,bool,int,int,int))

<a id="Std.Net.Sockets.TcpSocketNative.Connect(string,int,int)"></a>
### static extern long Connect(string host, int port, int timeoutMs)

[`src/libraries/z42.core/src/Native/NetNative.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L17)

连接 host:port，返回 socket 槽位。timeoutMs <= 0 → 不限时。

<a id="Std.Net.Sockets.TcpSocketNative.Listen(string,int,bool)"></a>
### static extern long Listen(string host, int port, bool reuseAddress)

[`src/libraries/z42.core/src/Native/NetNative.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L21)

绑定并监听，返回监听槽位。reuseAddress → bind 之前设 SO_REUSEADDR。

<a id="Std.Net.Sockets.TcpSocketNative.ListenerPort(long)"></a>
### static extern int ListenerPort(long listenerSlotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L25)

实际监听端口（Listen 传 0 时由 OS 分配）。

<a id="Std.Net.Sockets.TcpSocketNative.Accept(long)"></a>
### static extern long Accept(long listenerSlotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L29)

接受一个连接，返回 socket 槽位。

<a id="Std.Net.Sockets.TcpSocketNative.Read(long,byte[],int,int)"></a>
### static extern int Read(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L33)

读入 buffer，返回读到的字节数（0 = 对端已关闭）。

<a id="Std.Net.Sockets.TcpSocketNative.Write(long,byte[],int,int)"></a>
### static extern void Write(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L37)

写出 buffer。

<a id="Std.Net.Sockets.TcpSocketNative.Close(long)"></a>
### static extern void Close(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L41)

关闭 socket。

<a id="Std.Net.Sockets.TcpSocketNative.ListenerClose(long)"></a>
### static extern void ListenerClose(long listenerSlotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L45)

关闭监听 socket。

<a id="Std.Net.Sockets.TcpSocketNative.SetReadTimeout(long,int)"></a>
### static extern void SetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L49)

设置读超时；millis <= 0 清除超时（回到阻塞）。

<a id="Std.Net.Sockets.TcpSocketNative.SetWriteTimeout(long,int)"></a>
### static extern void SetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L53)

设置写超时；millis <= 0 清除超时（回到阻塞）。

<a id="Std.Net.Sockets.TcpSocketNative.SetNoDelay(long,bool)"></a>
### static extern void SetNoDelay(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L57)

设置 TCP_NODELAY（禁用 Nagle 算法）。

<a id="Std.Net.Sockets.TcpSocketNative.SetTtl(long,int)"></a>
### static extern void SetTtl(long slotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L61)

设置 IP_TTL。

<a id="Std.Net.Sockets.TcpSocketNative.ListenerSetTtl(long,int)"></a>
### static extern void ListenerSetTtl(long listenerSlotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L65)

设置监听 socket 的 IP_TTL。

<a id="Std.Net.Sockets.TcpSocketNative.SetKeepAlive(long,bool)"></a>
### static extern void SetKeepAlive(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L69)

设置 SO_KEEPALIVE。

<a id="Std.Net.Sockets.TcpSocketNative.SetKeepAliveTuned(long,bool,int,int,int)"></a>
### static extern void SetKeepAliveTuned(long slotId, bool enable, int idleSecs, int intervalSecs, int probes)

[`src/libraries/z42.core/src/Native/NetNative.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L73)

带调参的 SO_KEEPALIVE。

