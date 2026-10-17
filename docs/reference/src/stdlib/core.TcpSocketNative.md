<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TcpSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

`src/libraries/z42.core/src/Native/NetNative.z42:13`

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

### static extern long Connect(string host, int port, int timeoutMs)

`src/libraries/z42.core/src/Native/NetNative.z42:16`

连接 host:port，返回 socket 槽位。timeoutMs <= 0 → 不限时。

### static extern long Listen(string host, int port, bool reuseAddress)

`src/libraries/z42.core/src/Native/NetNative.z42:20`

绑定并监听，返回监听槽位。reuseAddress → bind 之前设 SO_REUSEADDR。

### static extern int ListenerPort(long listenerSlotId)

`src/libraries/z42.core/src/Native/NetNative.z42:24`

实际监听端口（Listen 传 0 时由 OS 分配）。

### static extern long Accept(long listenerSlotId)

`src/libraries/z42.core/src/Native/NetNative.z42:28`

接受一个连接，返回 socket 槽位。

### static extern int Read(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:32`

读入 buffer，返回读到的字节数（0 = 对端已关闭）。

### static extern void Write(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:36`

写出 buffer。

### static extern void Close(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:40`

关闭 socket。

### static extern void ListenerClose(long listenerSlotId)

`src/libraries/z42.core/src/Native/NetNative.z42:44`

关闭监听 socket。

### static extern void SetReadTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:48`

设置读超时；millis <= 0 清除超时（回到阻塞）。

### static extern void SetWriteTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:52`

设置写超时；millis <= 0 清除超时（回到阻塞）。

### static extern void SetNoDelay(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:56`

设置 TCP_NODELAY（禁用 Nagle 算法）。

### static extern void SetTtl(long slotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:60`

设置 IP_TTL。

### static extern void ListenerSetTtl(long listenerSlotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:64`

设置监听 socket 的 IP_TTL。

### static extern void SetKeepAlive(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:68`

设置 SO_KEEPALIVE。

### static extern void SetKeepAliveTuned(long slotId, bool enable, int idleSecs, int intervalSecs, int probes)

`src/libraries/z42.core/src/Native/NetNative.z42:72`

带调参的 SO_KEEPALIVE。

