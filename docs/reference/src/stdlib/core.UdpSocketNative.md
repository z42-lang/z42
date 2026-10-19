<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UdpSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L77)

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

成员：[`Bind`](#Std.Net.Sockets.UdpSocketNative.Bind(string,int)) · [`LocalPort`](#Std.Net.Sockets.UdpSocketNative.LocalPort(long)) · [`Send`](#Std.Net.Sockets.UdpSocketNative.Send(long,byte[],int,int,string,int)) · [`Receive`](#Std.Net.Sockets.UdpSocketNative.Receive(long)) · [`ReceiveInto`](#Std.Net.Sockets.UdpSocketNative.ReceiveInto(long,byte[],int,int)) · [`Close`](#Std.Net.Sockets.UdpSocketNative.Close(long)) · [`JoinMulticast`](#Std.Net.Sockets.UdpSocketNative.JoinMulticast(long,string,string)) · [`LeaveMulticast`](#Std.Net.Sockets.UdpSocketNative.LeaveMulticast(long,string,string)) · [`SetMulticastLoop`](#Std.Net.Sockets.UdpSocketNative.SetMulticastLoop(long,bool)) · [`SetTtl`](#Std.Net.Sockets.UdpSocketNative.SetTtl(long,int)) · [`SetReadTimeout`](#Std.Net.Sockets.UdpSocketNative.SetReadTimeout(long,int)) · [`SetWriteTimeout`](#Std.Net.Sockets.UdpSocketNative.SetWriteTimeout(long,int))

<a id="Std.Net.Sockets.UdpSocketNative.Bind(string,int)"></a>
### static extern long Bind(string host, int port)

[`src/libraries/z42.core/src/Native/NetNative.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L80)

绑定 host:port，返回 slot。

<a id="Std.Net.Sockets.UdpSocketNative.LocalPort(long)"></a>
### static extern int LocalPort(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L84)

本地绑定端口。

<a id="Std.Net.Sockets.UdpSocketNative.Send(long,byte[],int,int,string,int)"></a>
### static extern int Send(long slotId, byte[] buf, int offset, int count, string remoteHost, int remotePort)

[`src/libraries/z42.core/src/Native/NetNative.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L88)

向 remoteHost:remotePort 发送数据报。

<a id="Std.Net.Sockets.UdpSocketNative.Receive(long)"></a>
### static extern object[] Receive(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:92`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L92)

收一个数据报，返回 `[byte[] datagram, string remoteHost, long remotePort]`。

<a id="Std.Net.Sockets.UdpSocketNative.ReceiveInto(long,byte[],int,int)"></a>
### static extern object[] ReceiveInto(long slotId, byte[] buf, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L97)

收进调用方缓冲，返回 `[long bytesWritten, string remoteHost, long remotePort]`； 超出 `count` 的部分截断。

<a id="Std.Net.Sockets.UdpSocketNative.Close(long)"></a>
### static extern void Close(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L101)

关闭 socket。

<a id="Std.Net.Sockets.UdpSocketNative.JoinMulticast(long,string,string)"></a>
### static extern void JoinMulticast(long slotId, string groupIp, string ifaceIp)

[`src/libraries/z42.core/src/Native/NetNative.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L105)

加入 IPv4 组播组。

<a id="Std.Net.Sockets.UdpSocketNative.LeaveMulticast(long,string,string)"></a>
### static extern void LeaveMulticast(long slotId, string groupIp, string ifaceIp)

[`src/libraries/z42.core/src/Native/NetNative.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L109)

离开 IPv4 组播组。

<a id="Std.Net.Sockets.UdpSocketNative.SetMulticastLoop(long,bool)"></a>
### static extern void SetMulticastLoop(long slotId, bool enable)

[`src/libraries/z42.core/src/Native/NetNative.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L113)

设置 IP_MULTICAST_LOOP。

<a id="Std.Net.Sockets.UdpSocketNative.SetTtl(long,int)"></a>
### static extern void SetTtl(long slotId, int ttl)

[`src/libraries/z42.core/src/Native/NetNative.z42:117`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L117)

设置单播 IP_TTL。

<a id="Std.Net.Sockets.UdpSocketNative.SetReadTimeout(long,int)"></a>
### static extern void SetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L121)

设置读超时。

<a id="Std.Net.Sockets.UdpSocketNative.SetWriteTimeout(long,int)"></a>
### static extern void SetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L125)

设置写超时。

