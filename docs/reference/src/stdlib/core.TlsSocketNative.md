<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TlsSocketNative

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Net.Sockets`

[`src/libraries/z42.core/src/Native/NetNative.z42:128`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L128)

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

成员：[`Connect`](#Std.Net.Sockets.TlsSocketNative.Connect(string,int,int)) · [`Read`](#Std.Net.Sockets.TlsSocketNative.Read(long,byte[],int,int)) · [`Write`](#Std.Net.Sockets.TlsSocketNative.Write(long,byte[],int,int)) · [`Close`](#Std.Net.Sockets.TlsSocketNative.Close(long)) · [`SetReadTimeout`](#Std.Net.Sockets.TlsSocketNative.SetReadTimeout(long,int)) · [`SetWriteTimeout`](#Std.Net.Sockets.TlsSocketNative.SetWriteTimeout(long,int))

<a id="Std.Net.Sockets.TlsSocketNative.Connect(string,int,int)"></a>
### static extern long Connect(string host, int port, int timeoutMs)

[`src/libraries/z42.core/src/Native/NetNative.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L131)

连接并完成 TLS 握手，返回 slot。timeoutMs <= 0 → 连接与握手都不限时；> 0 同时约束两者。

<a id="Std.Net.Sockets.TlsSocketNative.Read(long,byte[],int,int)"></a>
### static extern int Read(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:135`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L135)

读入 buffer。

<a id="Std.Net.Sockets.TlsSocketNative.Write(long,byte[],int,int)"></a>
### static extern void Write(long slotId, byte[] buffer, int offset, int count)

[`src/libraries/z42.core/src/Native/NetNative.z42:139`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L139)

写出 buffer。

<a id="Std.Net.Sockets.TlsSocketNative.Close(long)"></a>
### static extern void Close(long slotId)

[`src/libraries/z42.core/src/Native/NetNative.z42:143`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L143)

关闭 TLS 会话。

<a id="Std.Net.Sockets.TlsSocketNative.SetReadTimeout(long,int)"></a>
### static extern void SetReadTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:147`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L147)

设置读超时。

<a id="Std.Net.Sockets.TlsSocketNative.SetWriteTimeout(long,int)"></a>
### static extern void SetWriteTimeout(long slotId, int millis)

[`src/libraries/z42.core/src/Native/NetNative.z42:151`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Native/NetNative.z42#L151)

设置写超时。

