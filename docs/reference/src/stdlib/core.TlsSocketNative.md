<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TlsSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

`src/libraries/z42.core/src/Native/NetNative.z42:128`

TLS socket 的 native 原语，供 z42.net 的 TlsClient 调用。

```z42
public static class TlsSocketNative {
    static extern long Connect(string host, int port, int timeoutMs)
    static extern int Read(long slotId, byte[] buffer, int offset, int count)
    static extern void Write(long slotId, byte[] buffer, int offset, int count)
    static extern void Close(long slotId)
    static extern void SetReadTimeout(long slotId, int millis)
    static extern void SetWriteTimeout(long slotId, int millis)
}
```

### static extern long Connect(string host, int port, int timeoutMs)

`src/libraries/z42.core/src/Native/NetNative.z42:131`

连接并完成 TLS 握手，返回 slot。timeoutMs <= 0 → 连接与握手都不限时；> 0 同时约束两者。

### static extern int Read(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:135`

读入 buffer。

### static extern void Write(long slotId, byte[] buffer, int offset, int count)

`src/libraries/z42.core/src/Native/NetNative.z42:139`

写出 buffer。

### static extern void Close(long slotId)

`src/libraries/z42.core/src/Native/NetNative.z42:143`

关闭 TLS 会话。

### static extern void SetReadTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:147`

设置读超时。

### static extern void SetWriteTimeout(long slotId, int millis)

`src/libraries/z42.core/src/Native/NetNative.z42:151`

设置写超时。

