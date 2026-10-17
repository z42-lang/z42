<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UdpNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

`src/libraries/z42.core/src/Native/NetNative.z42:262`

种子兼容（旧形状）：UDP socket 原语，返回判别数组。

```z42
public static class UdpNative {
    static extern object[] Bind(string host, int port)
    static extern object[] Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)
    static extern object[] Recv(long slotId)
    static extern void Drop(long slotId)
    static extern object[] RecvInto(long slotId, byte[] buf, int offset, int count)
    static extern object[] JoinMulticast(long slotId, string groupIp, string ifaceIp)
    static extern object[] LeaveMulticast(long slotId, string groupIp, string ifaceIp)
    static extern object[] SetMulticastLoop(long slotId, bool enable)
    static extern object[] SetTtl(long slotId, int ttl)
    static extern object[] SetReadTimeout(long slotId, int millis)
    static extern object[] SetWriteTimeout(long slotId, int millis)
}
```

### static extern object[] Bind(string host, int port)

`src/libraries/z42.core/src/Native/NetNative.z42:265`

（种子兼容）绑定。

### static extern object[] Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)

`src/libraries/z42.core/src/Native/NetNative.z42:269`

（种子兼容）发送，返回 `[0, bytesSent]` ok / `[1, msg]` err / `[2]` invalid。

### static extern object[] Recv(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:273`

（种子兼容）接收，返回 `[0, buf, remoteHost, remotePort]` ok / `[1, msg]` err / `[2]` invalid。

### static extern void Drop(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:277`

（种子兼容）关闭。

### static extern object[] RecvInto(long slotId, byte[] buf, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:281`

（种子兼容）收进调用方缓冲，返回判别数组；超出 `count` 截断。

### static extern object[] JoinMulticast(long slotId, string groupIp, string ifaceIp)

`src/libraries/z42.core/src/Native/NetNative.z42:285`

（种子兼容）加入 IPv4 组播组。

### static extern object[] LeaveMulticast(long slotId, string groupIp, string ifaceIp)

`src/libraries/z42.core/src/Native/NetNative.z42:289`

（种子兼容）离开 IPv4 组播组。

### static extern object[] SetMulticastLoop(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:293`

（种子兼容）设置 IP_MULTICAST_LOOP。

### static extern object[] SetTtl(long slotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:297`

（种子兼容）设置单播 IP_TTL。

### static extern object[] SetReadTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:301`

（种子兼容）设置读超时；millis <= 0 清除超时。

### static extern object[] SetWriteTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:305`

（种子兼容）设置写超时；millis <= 0 清除超时。

