<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UdpNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:262`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L262)

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

成员：[`Bind`](#Std.Net.Sockets.UdpNative.Bind(string,int)) · [`Send`](#Std.Net.Sockets.UdpNative.Send(long,byte[],int,int,string,int)) · [`Recv`](#Std.Net.Sockets.UdpNative.Recv(long)) · [`Drop`](#Std.Net.Sockets.UdpNative.Drop(long)) · [`RecvInto`](#Std.Net.Sockets.UdpNative.RecvInto(long,byte[],int,int)) · [`JoinMulticast`](#Std.Net.Sockets.UdpNative.JoinMulticast(long,string,string)) · [`LeaveMulticast`](#Std.Net.Sockets.UdpNative.LeaveMulticast(long,string,string)) · [`SetMulticastLoop`](#Std.Net.Sockets.UdpNative.SetMulticastLoop(long,bool)) · [`SetTtl`](#Std.Net.Sockets.UdpNative.SetTtl(long,int)) · [`SetReadTimeout`](#Std.Net.Sockets.UdpNative.SetReadTimeout(long,int)) · [`SetWriteTimeout`](#Std.Net.Sockets.UdpNative.SetWriteTimeout(long,int))

<a id="Std.Net.Sockets.UdpNative.Bind(string,int)"></a>
### static extern object[] Bind(string host, int port)

[`src/libraries/z42.core/src/Native/NetNative.z42:265`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L265)

（种子兼容）绑定。

<a id="Std.Net.Sockets.UdpNative.Send(long,byte[],int,int,string,int)"></a>
### static extern object[] Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)

[`src/libraries/z42.core/src/Native/NetNative.z42:269`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L269)

（种子兼容）发送，返回 `[0, bytesSent]` ok / `[1, msg]` err / `[2]` invalid。

<a id="Std.Net.Sockets.UdpNative.Recv(long)"></a>
### static extern object[] Recv(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:273`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L273)

（种子兼容）接收，返回 `[0, buf, remoteHost, remotePort]` ok / `[1, msg]` err / `[2]` invalid。

<a id="Std.Net.Sockets.UdpNative.Drop(long)"></a>
### static extern void Drop(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:277`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L277)

（种子兼容）关闭。

<a id="Std.Net.Sockets.UdpNative.RecvInto(long,byte[],int,int)"></a>
### static extern object[] RecvInto(long slotId, byte[] buf, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:281`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L281)

（种子兼容）收进调用方缓冲，返回判别数组；超出 `count` 截断。

<a id="Std.Net.Sockets.UdpNative.JoinMulticast(long,string,string)"></a>
### static extern object[] JoinMulticast(long slotId, string groupIp, string ifaceIp)

[`src/libraries/z42.core/src/Native/NetNative.z42:285`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L285)

（种子兼容）加入 IPv4 组播组。

<a id="Std.Net.Sockets.UdpNative.LeaveMulticast(long,string,string)"></a>
### static extern object[] LeaveMulticast(long slotId, string groupIp, string ifaceIp)

[`src/libraries/z42.core/src/Native/NetNative.z42:289`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L289)

（种子兼容）离开 IPv4 组播组。

<a id="Std.Net.Sockets.UdpNative.SetMulticastLoop(long,bool)"></a>
### static extern object[] SetMulticastLoop(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:293`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L293)

（种子兼容）设置 IP_MULTICAST_LOOP。

<a id="Std.Net.Sockets.UdpNative.SetTtl(long,int)"></a>
### static extern object[] SetTtl(long slotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:297`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L297)

（种子兼容）设置单播 IP_TTL。

<a id="Std.Net.Sockets.UdpNative.SetReadTimeout(long,int)"></a>
### static extern object[] SetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:301`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L301)

（种子兼容）设置读超时；millis <= 0 清除超时。

<a id="Std.Net.Sockets.UdpNative.SetWriteTimeout(long,int)"></a>
### static extern object[] SetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:305`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L305)

（种子兼容）设置写超时；millis <= 0 清除超时。

