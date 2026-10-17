<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.NetTcpNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

`src/libraries/z42.core/src/Native/NetNative.z42:166`

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

### static extern object[] Connect(string host, int port)

`src/libraries/z42.core/src/Native/NetNative.z42:169`

（种子兼容）连接。

### static extern object[] Listen(string host, int port)

`src/libraries/z42.core/src/Native/NetNative.z42:173`

（种子兼容）监听。

### static extern object[] Accept(long listenerSlotId)

`src/libraries/z42.core/src/Native/NetNative.z42:177`

（种子兼容）接受连接。

### static extern object[] SocketRead(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:181`

（种子兼容）读。

### static extern object[] SocketWrite(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:185`

（种子兼容）写。

### static extern void SocketDrop(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:189`

（种子兼容）关闭 socket。

### static extern void ListenerDrop(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:193`

（种子兼容）关闭监听 socket。

### static extern object[] SocketSetReadTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:197`

（种子兼容）设置读超时；millis <= 0 清除超时。

### static extern object[] SocketSetWriteTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:201`

（种子兼容）设置写超时；millis <= 0 清除超时。

### static extern object[] SocketSetNoDelay(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:205`

（种子兼容）设置 TCP_NODELAY。

### static extern object[] SocketSetTtl(long slotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:209`

（种子兼容）设置 IP_TTL。

### static extern object[] ListenerSetTtl(long slotId, int ttl)

`src/libraries/z42.core/src/Native/NetNative.z42:213`

（种子兼容）设置监听 IP_TTL。

### static extern object[] ConnectWithTimeout(string host, int port, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:217`

（种子兼容）带超时连接。

### static extern object[] SocketSetKeepalive(long slotId, bool enable)

`src/libraries/z42.core/src/Native/NetNative.z42:221`

（种子兼容）设置 SO_KEEPALIVE。

### static extern object[] SocketSetKeepaliveTuned(long slotId, bool enable, long idleSecs, long intervalSecs, long probes)

`src/libraries/z42.core/src/Native/NetNative.z42:225`

（种子兼容）带调参的 SO_KEEPALIVE。

### static extern object[] ListenWithOptions(string host, int port, bool reuseAddr)

`src/libraries/z42.core/src/Native/NetNative.z42:231`

（种子兼容）带选项监听。

