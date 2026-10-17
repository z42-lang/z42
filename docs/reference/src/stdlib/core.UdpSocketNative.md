<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UdpSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

`src/libraries/z42.core/src/Native/NetNative.z42:76`

UDP socket 的 native 原语，供 z42.net 的 UdpClient 调用。

```z42
public static class UdpSocketNative {
    static extern long Bind(string host, int port)
    static extern int LocalPort(long slotId)
    static extern int Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)
    static extern object[] Receive(long slotId)
    static extern object[] ReceiveInto(long slotId, byte[] buf, int offset, int count)
    static extern void Close(long slotId)
    static extern void JoinMulticast(long slotId, string groupIp, string ifaceIp)
    static extern void LeaveMulticast(long slotId, string groupIp, string ifaceIp)
    static extern void SetMulticastLoop(long slotId, bool enable)
    static extern void SetTtl(long slotId, int ttl)
    static extern void SetReadTimeout(long slotId, int millis)
    static extern void SetWriteTimeout(long slotId, int millis)
}
```

### static extern long Bind(string host, int port)

`src/libraries/z42.core/src/Native/NetNative.z42:79`

绑定 host:port，返回 slot。

### static extern int LocalPort(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:83`

本地绑定端口。

### static extern int Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)

`src/libraries/z42.core/src/Native/NetNative.z42:87`

向 remoteHost:remotePort 发送数据报。

### static extern object[] Receive(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:91`

收一个数据报，返回 `[byte[] datagram, string remoteHost, long remotePort]`。

### static extern object[] ReceiveInto(long slotId, byte[] buf, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:96`

收进调用方缓冲，返回 `[long bytesWritten, string remoteHost, long remotePort]`； 超出 `count` 的部分截断。

### static extern void Close(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:100`

关闭 socket。

### static extern void JoinMulticast(long slotId, string groupIp, string ifaceIp)

`src/libraries/z42.core/src/Native/NetNative.z42:104`

加入 IPv4 组播组。

### static extern void LeaveMulticast(long slotId, string groupIp, string ifaceIp)

`src/libraries/z42.core/src/Native/NetNative.z42:108`

离开 IPv4 组播组。

### static extern void SetMulticastLoop(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:112`

设置 IP_MULTICAST_LOOP。

### static extern void SetTtl(long slotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:116`

设置单播 IP_TTL。

### static extern void SetReadTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:120`

设置读超时。

### static extern void SetWriteTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:124`

设置写超时。

