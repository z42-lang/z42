<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.net/ -->

# z42.net —— 同步阻塞的网络类型。

> 包路径 `src/libraries/z42.net/`；命名空间 `Std.Net.Sockets`

网络栈：原始 socket（TCP / UDP）、TLS 客户端、HTTP/1.1 客户端与服务端、
WebSocket 客户端与服务端、DNS 解析、`IPAddress` / `IPEndPoint`。全部是同步阻塞
API——没有 async/await，没有非阻塞 socket，没有 selector；并发靠
`Std.Threading.Thread`（`HttpServer` 自带线程化与线程池两个入口）。超时以
`SetReadTimeout` / `SetWriteTimeout` / `SetConnectTimeout` 这类毫秒级 setter 表达，
不是每次调用的参数；`millis <= 0` 表示清除超时（回到无限阻塞）。

wasm32 目标上所有 socket 操作抛 `NetUnsupportedException`。

## 不支持

- async/await、非阻塞 socket、selector/epoll：一切都是阻塞调用 + 线程
- HTTP/2、HTTP/3：只有 HTTP/1.1
- 服务端 TLS：`HttpServer` 只跑明文 http；没有 `TlsListener`
- `wss://`：WebSocket 客户端只接 `ws://`
- WebSocket 服务端收消息：客户端帧带掩码，服务端解析不支持
- HTTPS 的流式响应体：`SendStreaming` 遇 `https://` 抛 `NotSupportedException`
- HTTPS 连接池：keep-alive 池只覆盖明文连接
- 客户端证书 / 自定义证书校验 / 系统信任库：只信内置 Mozilla 根集
- URL 里的 IPv6 字面量：`http://[::1]/` 解析失败
- URL 里的 userinfo：`http://u:p@h/` 抛 `NotSupportedException`
- IPv6 zone id：`fe80::1%en0` 解析失败
- `IPEndPoint` 作为 socket 参数：所有 socket API 收的是 `(host, port)`
- DNS 的记录类型查询与超时控制：只有 `GetHostAddresses`
- HTTP 服务端的路由、keep-alive、流式请求体、WebSocket upgrade
- 代理（HTTP_PROXY / SOCKS）
- wasm32 上的任何 socket：抛 `NetUnsupportedException`

## Dns

[`src/libraries/z42.net/src/Dns.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Dns.z42#L5)

同步阻塞的 DNS 查询（getaddrinfo）。

```z42
public static class Dns {
    static IPAddress[] GetHostAddresses(string host)
}
```

成员：[`GetHostAddresses`](#Std.Net.Sockets.Dns.GetHostAddresses(string))

<a id="Std.Net.Sockets.Dns.GetHostAddresses(string)"></a>
### static IPAddress[] GetHostAddresses(string host)

[`src/libraries/z42.net/src/Dns.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Dns.z42#L10)

把主机名解析为 IPAddress 数组。

解析失败抛 `SocketException`。数字字面量（如 "127.0.0.1" / "::1"）原样返回，
不走网络。

## SocketClosedException

[`src/libraries/z42.net/src/Exceptions/SocketClosedException.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketClosedException.z42#L6)

在已 `Dispose` / `Close` / `Stop` 的 socket 或 listener 上操作时抛出。

含阻塞中的 accept / 读写被另一线程关掉；继承 `ObjectDisposedException`。

```z42
public class SocketClosedException : ObjectDisposedException {
    SocketClosedException()
}
```

成员：[`SocketClosedException`](#Std.Net.Sockets.SocketClosedException.ctor())

<a id="Std.Net.Sockets.SocketClosedException.ctor()"></a>
### SocketClosedException()

[`src/libraries/z42.net/src/Exceptions/SocketClosedException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketClosedException.z42#L8)

无参构造：ObjectDisposedException 的对象名固定为 Std.Net.Sockets.Socket。

## SocketException

[`src/libraries/z42.net/src/Exceptions/SocketException.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketException.z42#L8)

socket 操作（connect / bind / accept / send / receive / 选项 / DNS）因系统 错误失败时抛出。

超时也是它（`SocketErrorCode == SocketError.TimedOut`），与 .NET 的
`Socket.Receive` 一致。

```z42
public class SocketException : NetException {
    SocketError SocketErrorCode
    int ErrorCode
    SocketException(string message)
    SocketException(int errorCode, string message)
}
```

成员：[`SocketErrorCode`](#Std.Net.Sockets.SocketException.SocketErrorCode) · [`ErrorCode`](#Std.Net.Sockets.SocketException.ErrorCode) · [`SocketException`](#Std.Net.Sockets.SocketException.ctor(string)) · [`SocketException`](#Std.Net.Sockets.SocketException.ctor(int,string))

<a id="Std.Net.Sockets.SocketException.SocketErrorCode"></a>
### SocketError SocketErrorCode

[`src/libraries/z42.net/src/Exceptions/SocketException.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketException.z42#L10)

结构化错误码（`SocketError`，Winsock 取值）。

<a id="Std.Net.Sockets.SocketException.ErrorCode"></a>
### int ErrorCode

[`src/libraries/z42.net/src/Exceptions/SocketException.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketException.z42#L12)

与 `SocketErrorCode` 同一个值的 int。

<a id="Std.Net.Sockets.SocketException.ctor(string)"></a>
### SocketException(string message)

[`src/libraries/z42.net/src/Exceptions/SocketException.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketException.z42#L15)

只带消息：错误码侧记为笼统的 SocketError / -1（拿不到系统码时用）。

<a id="Std.Net.Sockets.SocketException.ctor(int,string)"></a>
### SocketException(int errorCode, string message)

[`src/libraries/z42.net/src/Exceptions/SocketException.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/Exceptions/SocketException.z42#L21)

带系统错误码构造：`errorCode` 同时落到 SocketErrorCode 与 ErrorCode。

## IPAddress

[`src/libraries/z42.net/src/IPAddress.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L5)

强类型 IP 地址：解析、族判断与字节访问（IPv4 / IPv6）。

```z42
public sealed class IPAddress {
    static int FamilyIPv4
    static int FamilyIPv6
    IPAddress(int family, byte[] bytes)
    static IPAddress Loopback()
    static IPAddress Any()
    static IPAddress IPv6Loopback()
    static IPAddress IPv6Any()
    static IPAddress Parse(string s)
    static IPAddress? TryParse(string s)
    int Family()
    bool IsIPv4()
    bool IsIPv6()
    byte[] GetAddressBytes()
    bool IsLoopback()
    bool IsAny()
    bool IsMulticast()
    override string ToString()
    bool Equals(IPAddress other)
}
```

成员：[`FamilyIPv4`](#Std.Net.Sockets.IPAddress.FamilyIPv4) · [`FamilyIPv6`](#Std.Net.Sockets.IPAddress.FamilyIPv6) · [`IPAddress`](#Std.Net.Sockets.IPAddress.ctor(int,byte[])) · [`Loopback`](#Std.Net.Sockets.IPAddress.Loopback()) · [`Any`](#Std.Net.Sockets.IPAddress.Any()) · [`IPv6Loopback`](#Std.Net.Sockets.IPAddress.IPv6Loopback()) · [`IPv6Any`](#Std.Net.Sockets.IPAddress.IPv6Any()) · [`Parse`](#Std.Net.Sockets.IPAddress.Parse(string)) · [`TryParse`](#Std.Net.Sockets.IPAddress.TryParse(string)) · [`Family`](#Std.Net.Sockets.IPAddress.Family()) · [`IsIPv4`](#Std.Net.Sockets.IPAddress.IsIPv4()) · [`IsIPv6`](#Std.Net.Sockets.IPAddress.IsIPv6()) · [`GetAddressBytes`](#Std.Net.Sockets.IPAddress.GetAddressBytes()) · [`IsLoopback`](#Std.Net.Sockets.IPAddress.IsLoopback()) · [`IsAny`](#Std.Net.Sockets.IPAddress.IsAny()) · [`IsMulticast`](#Std.Net.Sockets.IPAddress.IsMulticast()) · [`ToString`](#Std.Net.Sockets.IPAddress.ToString()) · [`Equals`](#Std.Net.Sockets.IPAddress.Equals(IPAddress))

<a id="Std.Net.Sockets.IPAddress.FamilyIPv4"></a>
### static int FamilyIPv4

[`src/libraries/z42.net/src/IPAddress.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L7)

IPv4 的 family 常量值。

<a id="Std.Net.Sockets.IPAddress.FamilyIPv6"></a>
### static int FamilyIPv6

[`src/libraries/z42.net/src/IPAddress.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L9)

IPv6 的 family 常量值。

<a id="Std.Net.Sockets.IPAddress.ctor(int,byte[])"></a>
### IPAddress(int family, byte[] bytes)

[`src/libraries/z42.net/src/IPAddress.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L15)

以 family（4 或 6）与字节数组构造；family 4 需 4 字节、6 需 16 字节。

<a id="Std.Net.Sockets.IPAddress.Loopback()"></a>
### static IPAddress Loopback()

[`src/libraries/z42.net/src/IPAddress.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L34)

IPv4 回环地址（127.0.0.1）。

<a id="Std.Net.Sockets.IPAddress.Any()"></a>
### static IPAddress Any()

[`src/libraries/z42.net/src/IPAddress.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L41)

IPv4 任意地址（0.0.0.0）。

<a id="Std.Net.Sockets.IPAddress.IPv6Loopback()"></a>
### static IPAddress IPv6Loopback()

[`src/libraries/z42.net/src/IPAddress.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L46)

IPv6 回环地址（::1）。

<a id="Std.Net.Sockets.IPAddress.IPv6Any()"></a>
### static IPAddress IPv6Any()

[`src/libraries/z42.net/src/IPAddress.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L53)

IPv6 任意地址（::）。

<a id="Std.Net.Sockets.IPAddress.Parse(string)"></a>
### static IPAddress Parse(string s)

[`src/libraries/z42.net/src/IPAddress.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L60)

把字符串解析为 IPAddress；失败抛 `FormatException`。

<a id="Std.Net.Sockets.IPAddress.TryParse(string)"></a>
### static IPAddress? TryParse(string s)

[`src/libraries/z42.net/src/IPAddress.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L74)

非抛异常解析；不是合法的 IPv4 / IPv6 字面量时返回 null。

<a id="Std.Net.Sockets.IPAddress.Family()"></a>
### int Family()

[`src/libraries/z42.net/src/IPAddress.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L85)

返回地址族（4 或 6）。

<a id="Std.Net.Sockets.IPAddress.IsIPv4()"></a>
### bool IsIPv4()

[`src/libraries/z42.net/src/IPAddress.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L88)

是否 IPv4 地址。

<a id="Std.Net.Sockets.IPAddress.IsIPv6()"></a>
### bool IsIPv6()

[`src/libraries/z42.net/src/IPAddress.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L90)

是否 IPv6 地址。

<a id="Std.Net.Sockets.IPAddress.GetAddressBytes()"></a>
### byte[] GetAddressBytes()

[`src/libraries/z42.net/src/IPAddress.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L93)

返回原始字节的副本（IPv4 为 4 字节、IPv6 为 16 字节）。

<a id="Std.Net.Sockets.IPAddress.IsLoopback()"></a>
### bool IsLoopback()

[`src/libraries/z42.net/src/IPAddress.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L101)

是否回环地址（IPv4 的 127.0.0.0/8 或 IPv6 的 ::1）。

<a id="Std.Net.Sockets.IPAddress.IsAny()"></a>
### bool IsAny()

[`src/libraries/z42.net/src/IPAddress.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L115)

是否任意地址（IPv4 的 0.0.0.0 或 IPv6 的 ::）。

<a id="Std.Net.Sockets.IPAddress.IsMulticast()"></a>
### bool IsMulticast()

[`src/libraries/z42.net/src/IPAddress.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L125)

是否组播地址（IPv4 的 224.0.0.0/4 或 IPv6 的 ff00::/8）。

<a id="Std.Net.Sockets.IPAddress.ToString()"></a>
### override string ToString()

[`src/libraries/z42.net/src/IPAddress.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L136)

规范字符串；IPv4 用点分十进制，IPv6 用最长零段折叠（小写 hex）。

<a id="Std.Net.Sockets.IPAddress.Equals(IPAddress)"></a>
### bool Equals(IPAddress other)

[`src/libraries/z42.net/src/IPAddress.z42:147`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPAddress.z42#L147)

与另一个 IPAddress 比较是否相等。

## IPEndPoint

[`src/libraries/z42.net/src/IPEndPoint.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L5)

`(IPAddress, port)` 的值对象，支持 canonical `addr:port` / `[addr]:port` 解析与显示。

```z42
public sealed class IPEndPoint {
    static int MinPort
    static int MaxPort
    IPEndPoint(IPAddress address, int port)
    IPAddress Address()
    int Port()
    override string ToString()
    static IPEndPoint Parse(string s)
    bool Equals(IPEndPoint other)
}
```

成员：[`MinPort`](#Std.Net.Sockets.IPEndPoint.MinPort) · [`MaxPort`](#Std.Net.Sockets.IPEndPoint.MaxPort) · [`IPEndPoint`](#Std.Net.Sockets.IPEndPoint.ctor(IPAddress,int)) · [`Address`](#Std.Net.Sockets.IPEndPoint.Address()) · [`Port`](#Std.Net.Sockets.IPEndPoint.Port()) · [`ToString`](#Std.Net.Sockets.IPEndPoint.ToString()) · [`Parse`](#Std.Net.Sockets.IPEndPoint.Parse(string)) · [`Equals`](#Std.Net.Sockets.IPEndPoint.Equals(IPEndPoint))

<a id="Std.Net.Sockets.IPEndPoint.MinPort"></a>
### static int MinPort

[`src/libraries/z42.net/src/IPEndPoint.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L7)

最小端口值。

<a id="Std.Net.Sockets.IPEndPoint.MaxPort"></a>
### static int MaxPort

[`src/libraries/z42.net/src/IPEndPoint.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L9)

最大端口值。

<a id="Std.Net.Sockets.IPEndPoint.ctor(IPAddress,int)"></a>
### IPEndPoint(IPAddress address, int port)

[`src/libraries/z42.net/src/IPEndPoint.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L15)

以地址和端口构造；port 越界抛 `ArgumentException`。

<a id="Std.Net.Sockets.IPEndPoint.Address()"></a>
### IPAddress Address()

[`src/libraries/z42.net/src/IPEndPoint.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L28)

返回地址。

<a id="Std.Net.Sockets.IPEndPoint.Port()"></a>
### int Port()

[`src/libraries/z42.net/src/IPEndPoint.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L30)

返回端口。

<a id="Std.Net.Sockets.IPEndPoint.ToString()"></a>
### override string ToString()

[`src/libraries/z42.net/src/IPEndPoint.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L33)

规范形式：IPv4 → `addr:port`；IPv6 → `[addr]:port`。

<a id="Std.Net.Sockets.IPEndPoint.Parse(string)"></a>
### static IPEndPoint Parse(string s)

[`src/libraries/z42.net/src/IPEndPoint.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L41)

解析 `addr:port`（IPv4）或 `[addr]:port`（IPv6）形式；失败抛 `FormatException`。

<a id="Std.Net.Sockets.IPEndPoint.Equals(IPEndPoint)"></a>
### bool Equals(IPEndPoint other)

[`src/libraries/z42.net/src/IPEndPoint.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/IPEndPoint.z42#L96)

与另一个 IPEndPoint 比较是否相等。

## NetworkStream

[`src/libraries/z42.net/src/NetworkStream.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/NetworkStream.z42#L6)

TCP socket 之上的字节流，`Std.IO.Stream` 的子类，由 `TcpClient.GetStream()` 给出。

```z42
public class NetworkStream : Stream {
    NetworkStream(long slotId)
    void MarkClosed()
}
```

成员：[`NetworkStream`](#Std.Net.Sockets.NetworkStream.ctor(long)) · [`MarkClosed`](#Std.Net.Sockets.NetworkStream.MarkClosed())

<a id="Std.Net.Sockets.NetworkStream.ctor(long)"></a>
### NetworkStream(long slotId)

[`src/libraries/z42.net/src/NetworkStream.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/NetworkStream.z42#L11)

以底层 TCP socket 的 slot 句柄包装流；初始为打开状态。

<a id="Std.Net.Sockets.NetworkStream.MarkClosed()"></a>
### void MarkClosed()

[`src/libraries/z42.net/src/NetworkStream.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/NetworkStream.z42#L70)

翻转 _closed 位使本流失效；幂等，底层 fd 由 TcpClient.Dispose 释放。

## SocketError

[`src/libraries/z42.net/src/SocketError.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/SocketError.z42#L6)

socket 操作的错误码，镜像 .NET 的 `SocketError`，取值即 Winsock 码。

读 `SocketException.SocketErrorCode`。

```z42
public enum SocketError {
    SocketError = -1
    Success = 0
    OperationAborted = 995
    IOPending = 997
    Interrupted = 10004
    AccessDenied = 10013
    Fault = 10014
    InvalidArgument = 10022
    TooManyOpenSockets = 10024
    WouldBlock = 10035
    InProgress = 10036
    AlreadyInProgress = 10037
    NotSocket = 10038
    DestinationAddressRequired = 10039
    MessageSize = 10040
    ProtocolType = 10041
    ProtocolOption = 10042
    ProtocolNotSupported = 10043
    SocketNotSupported = 10044
    OperationNotSupported = 10045
    ProtocolFamilyNotSupported = 10046
    AddressFamilyNotSupported = 10047
    AddressAlreadyInUse = 10048
    AddressNotAvailable = 10049
    NetworkDown = 10050
    NetworkUnreachable = 10051
    NetworkReset = 10052
    ConnectionAborted = 10053
    ConnectionReset = 10054
    NoBufferSpaceAvailable = 10055
    IsConnected = 10056
    NotConnected = 10057
    Shutdown = 10058
    TimedOut = 10060
    ConnectionRefused = 10061
    HostDown = 10064
    HostUnreachable = 10065
    ProcessLimit = 10067
    SystemNotReady = 10091
    VersionNotSupported = 10092
    NotInitialized = 10093
    Disconnecting = 10101
    TypeNotFound = 10109
    HostNotFound = 11001
    TryAgain = 11002
    NoRecovery = 11003
    NoData = 11004
}
```

## TcpClient

[`src/libraries/z42.net/src/TcpClient.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L34)

同步阻塞的 TCP 客户端，对应 .NET 的 `TcpClient`。

```z42
public class TcpClient : IDisposable {
    TcpClient()
    static TcpClient ConnectTo(string host, int port)
    void Connect(string host, int port)
    void SetConnectTimeout(int millis)
    NetworkStream GetStream()
    string RemoteHost()
    int RemotePort()
    void SetReadTimeout(int millis)
    void SetWriteTimeout(int millis)
    void SetNoDelay(bool enable)
    void SetTtl(int ttl)
    void SetKeepAlive(bool enable)
    void SetKeepAlive(bool enable, int idleSecs, int intervalSecs, int probes)
    void Dispose()
    void Close()
    TcpClient(long slotId, bool fromAccept)
}
```

成员：[`TcpClient`](#Std.Net.Sockets.TcpClient.ctor()) · [`ConnectTo`](#Std.Net.Sockets.TcpClient.ConnectTo(string,int)) · [`Connect`](#Std.Net.Sockets.TcpClient.Connect(string,int)) · [`SetConnectTimeout`](#Std.Net.Sockets.TcpClient.SetConnectTimeout(int)) · [`GetStream`](#Std.Net.Sockets.TcpClient.GetStream()) · [`RemoteHost`](#Std.Net.Sockets.TcpClient.RemoteHost()) · [`RemotePort`](#Std.Net.Sockets.TcpClient.RemotePort()) · [`SetReadTimeout`](#Std.Net.Sockets.TcpClient.SetReadTimeout(int)) · [`SetWriteTimeout`](#Std.Net.Sockets.TcpClient.SetWriteTimeout(int)) · [`SetNoDelay`](#Std.Net.Sockets.TcpClient.SetNoDelay(bool)) · [`SetTtl`](#Std.Net.Sockets.TcpClient.SetTtl(int)) · [`SetKeepAlive`](#Std.Net.Sockets.TcpClient.SetKeepAlive(bool)) · [`SetKeepAlive`](#Std.Net.Sockets.TcpClient.SetKeepAlive(bool,int,int,int)) · [`Dispose`](#Std.Net.Sockets.TcpClient.Dispose()) · [`Close`](#Std.Net.Sockets.TcpClient.Close()) · [`TcpClient`](#Std.Net.Sockets.TcpClient.ctor(long,bool))

<a id="Std.Net.Sockets.TcpClient.ctor()"></a>
### TcpClient()

[`src/libraries/z42.net/src/TcpClient.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L45)

构造未连接实例；随后 `Connect(host, port)` 建立连接。

<a id="Std.Net.Sockets.TcpClient.ConnectTo(string,int)"></a>
### static TcpClient ConnectTo(string host, int port)

[`src/libraries/z42.net/src/TcpClient.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L56)

等价于 `new TcpClient()` 后再 `Connect(host, port)` 的静态工厂。

<a id="Std.Net.Sockets.TcpClient.Connect(string,int)"></a>
### void Connect(string host, int port)

[`src/libraries/z42.net/src/TcpClient.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L64)

建立到 `host:port` 的 TCP 连接，阻塞直到握手完成或失败。 连接被拒 / DNS 失败 / 网络不可达抛 `SocketException`。

<a id="Std.Net.Sockets.TcpClient.SetConnectTimeout(int)"></a>
### void SetConnectTimeout(int millis)

[`src/libraries/z42.net/src/TcpClient.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L81)

设置下一次 `Connect` 的超时（毫秒）；`millis <= 0` 清除超时。 只作用于紧随其后的一次 Connect；该值在同一实例上跨重连持续有效。

<a id="Std.Net.Sockets.TcpClient.GetStream()"></a>
### NetworkStream GetStream()

[`src/libraries/z42.net/src/TcpClient.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L86)

返回该连接的字节流 `NetworkStream`；惰性创建并缓存，重复调用返回同一实例。

<a id="Std.Net.Sockets.TcpClient.RemoteHost()"></a>
### string RemoteHost()

[`src/libraries/z42.net/src/TcpClient.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L100)

返回连接的远端主机。

<a id="Std.Net.Sockets.TcpClient.RemotePort()"></a>
### int RemotePort()

[`src/libraries/z42.net/src/TcpClient.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L105)

返回连接的远端端口。

<a id="Std.Net.Sockets.TcpClient.SetReadTimeout(int)"></a>
### void SetReadTimeout(int millis)

[`src/libraries/z42.net/src/TcpClient.z42:110`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L110)

设置读超时（毫秒）；`millis <= 0` 清除超时。

<a id="Std.Net.Sockets.TcpClient.SetWriteTimeout(int)"></a>
### void SetWriteTimeout(int millis)

[`src/libraries/z42.net/src/TcpClient.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L121)

设置写超时（毫秒）；`millis <= 0` 清除超时。

<a id="Std.Net.Sockets.TcpClient.SetNoDelay(bool)"></a>
### void SetNoDelay(bool enable)

[`src/libraries/z42.net/src/TcpClient.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L132)

设置 TCP_NODELAY（禁用 Nagle 算法）。

<a id="Std.Net.Sockets.TcpClient.SetTtl(int)"></a>
### void SetTtl(int ttl)

[`src/libraries/z42.net/src/TcpClient.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L141)

设置 IP_TTL（跳数限制），`ttl` 通常在 [1, 255]。

<a id="Std.Net.Sockets.TcpClient.SetKeepAlive(bool)"></a>
### void SetKeepAlive(bool enable)

[`src/libraries/z42.net/src/TcpClient.z42:150`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L150)

开启或关闭 SO_KEEPALIVE（使用系统默认的 idle / interval / probe 参数）。

<a id="Std.Net.Sockets.TcpClient.SetKeepAlive(bool,int,int,int)"></a>
### void SetKeepAlive(bool enable, int idleSecs, int intervalSecs, int probes)

[`src/libraries/z42.net/src/TcpClient.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L159)

带调参的 SO_KEEPALIVE；`enable == true` 时三个调参值都必须 >= 1。

<a id="Std.Net.Sockets.TcpClient.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.net/src/TcpClient.z42:169`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L169)

关闭 socket 并释放底层 fd；幂等。之后的操作抛 `SocketClosedException`。

<a id="Std.Net.Sockets.TcpClient.Close()"></a>
### void Close()

[`src/libraries/z42.net/src/TcpClient.z42:182`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L182)

`Dispose()` 的别名，对齐 .NET 的 `TcpClient.Close()`。

<a id="Std.Net.Sockets.TcpClient.ctor(long,bool)"></a>
### TcpClient(long slotId, bool fromAccept)

[`src/libraries/z42.net/src/TcpClient.z42:189`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpClient.z42#L189)

以 accept 到的 slot 句柄直接包装成已连接客户端（远端记为 "<accepted>"）。

## TcpListener

[`src/libraries/z42.net/src/TcpListener.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L5)

同步阻塞的 TCP 服务端，对应 .NET 的 `TcpListener`。

```z42
public class TcpListener : IDisposable {
    TcpListener()
    static TcpListener Create(string host, int port)
    void Bind(string host, int port)
    void SetReuseAddress(bool enable)
    void Start()
    TcpClient AcceptTcpClient()
    int LocalPort()
    string BindHost()
    void SetTtl(int ttl)
    void Stop()
    void Dispose()
}
```

成员：[`TcpListener`](#Std.Net.Sockets.TcpListener.ctor()) · [`Create`](#Std.Net.Sockets.TcpListener.Create(string,int)) · [`Bind`](#Std.Net.Sockets.TcpListener.Bind(string,int)) · [`SetReuseAddress`](#Std.Net.Sockets.TcpListener.SetReuseAddress(bool)) · [`Start`](#Std.Net.Sockets.TcpListener.Start()) · [`AcceptTcpClient`](#Std.Net.Sockets.TcpListener.AcceptTcpClient()) · [`LocalPort`](#Std.Net.Sockets.TcpListener.LocalPort()) · [`BindHost`](#Std.Net.Sockets.TcpListener.BindHost()) · [`SetTtl`](#Std.Net.Sockets.TcpListener.SetTtl(int)) · [`Stop`](#Std.Net.Sockets.TcpListener.Stop()) · [`Dispose`](#Std.Net.Sockets.TcpListener.Dispose())

<a id="Std.Net.Sockets.TcpListener.ctor()"></a>
### TcpListener()

[`src/libraries/z42.net/src/TcpListener.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L15)

构造未绑定实例；随后 `Bind(host, port)` 开始监听。

<a id="Std.Net.Sockets.TcpListener.Create(string,int)"></a>
### static TcpListener Create(string host, int port)

[`src/libraries/z42.net/src/TcpListener.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L25)

等价于 `new TcpListener()` 后 `Bind(host, port)` 的静态工厂。

<a id="Std.Net.Sockets.TcpListener.Bind(string,int)"></a>
### void Bind(string host, int port)

[`src/libraries/z42.net/src/TcpListener.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L32)

绑定到 `host:port` 并开始监听；port 0 表示由 OS 分配端口（事后用 `LocalPort()` 查询）。

<a id="Std.Net.Sockets.TcpListener.SetReuseAddress(bool)"></a>
### void SetReuseAddress(bool enable)

[`src/libraries/z42.net/src/TcpListener.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L48)

设置 SO_REUSEADDR（只能在 `Bind` 之前调用）；Bind 之后再调抛 `InvalidOperationException`。

<a id="Std.Net.Sockets.TcpListener.Start()"></a>
### void Start()

[`src/libraries/z42.net/src/TcpListener.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L57)

BCL 兼容的空操作；`Bind` 已经原子地完成了 bind + listen。

<a id="Std.Net.Sockets.TcpListener.AcceptTcpClient()"></a>
### TcpClient AcceptTcpClient()

[`src/libraries/z42.net/src/TcpListener.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L65)

阻塞直到有对端连接，返回包装已接受 socket 的 `TcpClient`。

<a id="Std.Net.Sockets.TcpListener.LocalPort()"></a>
### int LocalPort()

[`src/libraries/z42.net/src/TcpListener.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L77)

本地绑定端口 —— 用 port 0 绑定时查询 OS 分配到的端口。

<a id="Std.Net.Sockets.TcpListener.BindHost()"></a>
### string BindHost()

[`src/libraries/z42.net/src/TcpListener.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L82)

返回绑定主机。

<a id="Std.Net.Sockets.TcpListener.SetTtl(int)"></a>
### void SetTtl(int ttl)

[`src/libraries/z42.net/src/TcpListener.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L87)

设置监听 socket 的 IP_TTL（Bind 之后调用）。

<a id="Std.Net.Sockets.TcpListener.Stop()"></a>
### void Stop()

[`src/libraries/z42.net/src/TcpListener.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L96)

停止接受并释放监听 socket；幂等。

<a id="Std.Net.Sockets.TcpListener.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.net/src/TcpListener.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TcpListener.z42#L101)

释放监听 socket；幂等。

## TlsStream

[`src/libraries/z42.net/src/TlsClient.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L6)

TLS 会话之上的字节流（rustls 客户端侧），`NetworkStream` 的 TLS 版。

```z42
public class TlsStream : Stream {
    TlsStream(long slotId)
    void MarkClosed()
}
```

成员：[`TlsStream`](#Std.Net.Sockets.TlsStream.ctor(long)) · [`MarkClosed`](#Std.Net.Sockets.TlsStream.MarkClosed())

<a id="Std.Net.Sockets.TlsStream.ctor(long)"></a>
### TlsStream(long slotId)

[`src/libraries/z42.net/src/TlsClient.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L11)

以底层 TLS 会话的 slot 句柄包装流；初始为打开状态。

<a id="Std.Net.Sockets.TlsStream.MarkClosed()"></a>
### void MarkClosed()

[`src/libraries/z42.net/src/TlsClient.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L53)

翻转 _closed 位使本流失效；幂等，底层会话由 TlsClient.Dispose 释放。

## TlsClient

[`src/libraries/z42.net/src/TlsClient.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L65)

同步阻塞的 TLS 客户端；证书校验始终开启（rustls + 内置 Mozilla 根集，SNI = 连接主机）。

```z42
public class TlsClient : IDisposable {
    TlsClient()
    void Connect(string host, int port, int timeoutMs)
    Stream GetStream()
    string RemoteHost()
    int RemotePort()
    void SetReadTimeout(int millis)
    void SetWriteTimeout(int millis)
    void Dispose()
    void Close()
}
```

成员：[`TlsClient`](#Std.Net.Sockets.TlsClient.ctor()) · [`Connect`](#Std.Net.Sockets.TlsClient.Connect(string,int,int)) · [`GetStream`](#Std.Net.Sockets.TlsClient.GetStream()) · [`RemoteHost`](#Std.Net.Sockets.TlsClient.RemoteHost()) · [`RemotePort`](#Std.Net.Sockets.TlsClient.RemotePort()) · [`SetReadTimeout`](#Std.Net.Sockets.TlsClient.SetReadTimeout(int)) · [`SetWriteTimeout`](#Std.Net.Sockets.TlsClient.SetWriteTimeout(int)) · [`Dispose`](#Std.Net.Sockets.TlsClient.Dispose()) · [`Close`](#Std.Net.Sockets.TlsClient.Close())

<a id="Std.Net.Sockets.TlsClient.ctor()"></a>
### TlsClient()

[`src/libraries/z42.net/src/TlsClient.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L74)

构造未连接实例；随后 `Connect(host, port, timeoutMs)` 建立会话。

<a id="Std.Net.Sockets.TlsClient.Connect(string,int,int)"></a>
### void Connect(string host, int port, int timeoutMs)

[`src/libraries/z42.net/src/TlsClient.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L84)

建立到 `host:port` 的 TCP 连接并完成 TLS 握手；`timeoutMs > 0` 同时约束两者。

<a id="Std.Net.Sockets.TlsClient.GetStream()"></a>
### Stream GetStream()

[`src/libraries/z42.net/src/TlsClient.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L99)

返回该会话的字节流（具体类型为 `TlsStream`）；惰性创建并缓存。

<a id="Std.Net.Sockets.TlsClient.RemoteHost()"></a>
### string RemoteHost()

[`src/libraries/z42.net/src/TlsClient.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L113)

返回连接的远端主机。

<a id="Std.Net.Sockets.TlsClient.RemotePort()"></a>
### int RemotePort()

[`src/libraries/z42.net/src/TlsClient.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L118)

返回连接的远端端口。

<a id="Std.Net.Sockets.TlsClient.SetReadTimeout(int)"></a>
### void SetReadTimeout(int millis)

[`src/libraries/z42.net/src/TlsClient.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L123)

设置读超时（毫秒）；`millis <= 0` 清除超时。

<a id="Std.Net.Sockets.TlsClient.SetWriteTimeout(int)"></a>
### void SetWriteTimeout(int millis)

[`src/libraries/z42.net/src/TlsClient.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L134)

设置写超时（毫秒）；`millis <= 0` 清除超时。

<a id="Std.Net.Sockets.TlsClient.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.net/src/TlsClient.z42:145`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L145)

关闭会话并释放底层 fd；幂等。

<a id="Std.Net.Sockets.TlsClient.Close()"></a>
### void Close()

[`src/libraries/z42.net/src/TlsClient.z42:158`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/TlsClient.z42#L158)

`Dispose()` 的别名，对齐 `TcpClient.Close()`。

## UdpClient

[`src/libraries/z42.net/src/UdpClient.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L5)

同步阻塞的 UDP（数据报）socket，对应 .NET 的 `UdpClient`。

```z42
public class UdpClient : IDisposable {
    UdpClient()
    void Bind(string host, int port)
    int Send(byte[] data, int length, string remoteHost, int remotePort)
    UdpReceiveResult Receive()
    void Connect(string remoteHost, int remotePort)
    void Disconnect()
    bool IsConnected()
    int SendConnected(byte[] data, int length)
    int SendConnected(byte[] data)
    int ReceiveInto(byte[] buf, int offset, int count)
    int ReceiveIntoFull(byte[] buf)
    string LastReceiveHost()
    int LastReceivePort()
    void JoinMulticastGroup(string groupIp, string ifaceIp)
    void JoinMulticastGroup(string groupIp)
    void LeaveMulticastGroup(string groupIp, string ifaceIp)
    void LeaveMulticastGroup(string groupIp)
    void SetMulticastLoop(bool enable)
    void SetTtl(int ttl)
    void SetReadTimeout(int millis)
    void SetWriteTimeout(int millis)
    int LocalPort()
    string BindHost()
    void Dispose()
    void Close()
}
```

成员：[`UdpClient`](#Std.Net.Sockets.UdpClient.ctor()) · [`Bind`](#Std.Net.Sockets.UdpClient.Bind(string,int)) · [`Send`](#Std.Net.Sockets.UdpClient.Send(byte[],int,string,int)) · [`Receive`](#Std.Net.Sockets.UdpClient.Receive()) · [`Connect`](#Std.Net.Sockets.UdpClient.Connect(string,int)) · [`Disconnect`](#Std.Net.Sockets.UdpClient.Disconnect()) · [`IsConnected`](#Std.Net.Sockets.UdpClient.IsConnected()) · [`SendConnected`](#Std.Net.Sockets.UdpClient.SendConnected(byte[],int)) · [`SendConnected`](#Std.Net.Sockets.UdpClient.SendConnected(byte[])) · [`ReceiveInto`](#Std.Net.Sockets.UdpClient.ReceiveInto(byte[],int,int)) · [`ReceiveIntoFull`](#Std.Net.Sockets.UdpClient.ReceiveIntoFull(byte[])) · [`LastReceiveHost`](#Std.Net.Sockets.UdpClient.LastReceiveHost()) · [`LastReceivePort`](#Std.Net.Sockets.UdpClient.LastReceivePort()) · [`JoinMulticastGroup`](#Std.Net.Sockets.UdpClient.JoinMulticastGroup(string,string)) · [`JoinMulticastGroup`](#Std.Net.Sockets.UdpClient.JoinMulticastGroup(string)) · [`LeaveMulticastGroup`](#Std.Net.Sockets.UdpClient.LeaveMulticastGroup(string,string)) · [`LeaveMulticastGroup`](#Std.Net.Sockets.UdpClient.LeaveMulticastGroup(string)) · [`SetMulticastLoop`](#Std.Net.Sockets.UdpClient.SetMulticastLoop(bool)) · [`SetTtl`](#Std.Net.Sockets.UdpClient.SetTtl(int)) · [`SetReadTimeout`](#Std.Net.Sockets.UdpClient.SetReadTimeout(int)) · [`SetWriteTimeout`](#Std.Net.Sockets.UdpClient.SetWriteTimeout(int)) · [`LocalPort`](#Std.Net.Sockets.UdpClient.LocalPort()) · [`BindHost`](#Std.Net.Sockets.UdpClient.BindHost()) · [`Dispose`](#Std.Net.Sockets.UdpClient.Dispose()) · [`Close`](#Std.Net.Sockets.UdpClient.Close())

<a id="Std.Net.Sockets.UdpClient.ctor()"></a>
### UdpClient()

[`src/libraries/z42.net/src/UdpClient.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L20)

构造未绑定实例；`Bind` 显式绑定，或首次 `Send` 自动绑定临时端口。

<a id="Std.Net.Sockets.UdpClient.Bind(string,int)"></a>
### void Bind(string host, int port)

[`src/libraries/z42.net/src/UdpClient.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L34)

绑定到 `host:port`；port 0 由 OS 分配临时端口（事后用 `LocalPort()` 查询）。

<a id="Std.Net.Sockets.UdpClient.Send(byte[],int,string,int)"></a>
### int Send(byte[] data, int length, string remoteHost, int remotePort)

[`src/libraries/z42.net/src/UdpClient.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L51)

向 `remoteHost:remotePort` 发送 `data[0..length]` 的数据报，返回发送字节数。

若尚未 Bind，则自动绑定到 ("0.0.0.0", 0)。

<a id="Std.Net.Sockets.UdpClient.Receive()"></a>
### UdpReceiveResult Receive()

[`src/libraries/z42.net/src/UdpClient.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L63)

阻塞直到收到一个数据报，返回数据报及其发送方地址。

<a id="Std.Net.Sockets.UdpClient.Connect(string,int)"></a>
### void Connect(string remoteHost, int remotePort)

[`src/libraries/z42.net/src/UdpClient.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L80)

设置默认远端地址，供后续 `SendConnected` 使用；不调用 OS 的 connect(2)。

<a id="Std.Net.Sockets.UdpClient.Disconnect()"></a>
### void Disconnect()

[`src/libraries/z42.net/src/UdpClient.z42:90`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L90)

清除默认远端地址；幂等。

<a id="Std.Net.Sockets.UdpClient.IsConnected()"></a>
### bool IsConnected()

[`src/libraries/z42.net/src/UdpClient.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L97)

是否已 Connect 且未 Disconnect。

<a id="Std.Net.Sockets.UdpClient.SendConnected(byte[],int)"></a>
### int SendConnected(byte[] data, int length)

[`src/libraries/z42.net/src/UdpClient.z42:102`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L102)

向 `Connect` 设置的远端发送 `data[0..length]`；未 Connect 抛 `InvalidOperationException`。

<a id="Std.Net.Sockets.UdpClient.SendConnected(byte[])"></a>
### int SendConnected(byte[] data)

[`src/libraries/z42.net/src/UdpClient.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L114)

向已连接的远端发送整个 `data` 数组。

<a id="Std.Net.Sockets.UdpClient.ReceiveInto(byte[],int,int)"></a>
### int ReceiveInto(byte[] buf, int offset, int count)

[`src/libraries/z42.net/src/UdpClient.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L121)

把数据报收进调用方缓冲 `buf[offset..offset+count)`，省一次分配。

返回写入字节数。对端地址通过 `LastReceiveHost` / `LastReceivePort` 读取。

<a id="Std.Net.Sockets.UdpClient.ReceiveIntoFull(byte[])"></a>
### int ReceiveIntoFull(byte[] buf)

[`src/libraries/z42.net/src/UdpClient.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L136)

把数据报收满整个 `buf`（offset 0、count buf.Length）。

<a id="Std.Net.Sockets.UdpClient.LastReceiveHost()"></a>
### string LastReceiveHost()

[`src/libraries/z42.net/src/UdpClient.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L141)

最近一次 `ReceiveInto*` 的对端主机（成功接收前为空）。

<a id="Std.Net.Sockets.UdpClient.LastReceivePort()"></a>
### int LastReceivePort()

[`src/libraries/z42.net/src/UdpClient.z42:144`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L144)

最近一次 `ReceiveInto*` 的对端端口（成功接收前为 0）。

<a id="Std.Net.Sockets.UdpClient.JoinMulticastGroup(string,string)"></a>
### void JoinMulticastGroup(string groupIp, string ifaceIp)

[`src/libraries/z42.net/src/UdpClient.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L149)

加入 IPv4 组播组；`ifaceIp` 传 "0.0.0.0" 表示由 OS 选择接口。

<a id="Std.Net.Sockets.UdpClient.JoinMulticastGroup(string)"></a>
### void JoinMulticastGroup(string groupIp)

[`src/libraries/z42.net/src/UdpClient.z42:158`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L158)

加入 IPv4 组播组，接口交给 OS（0.0.0.0）。

<a id="Std.Net.Sockets.UdpClient.LeaveMulticastGroup(string,string)"></a>
### void LeaveMulticastGroup(string groupIp, string ifaceIp)

[`src/libraries/z42.net/src/UdpClient.z42:163`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L163)

离开之前加入的 IPv4 组播组。

<a id="Std.Net.Sockets.UdpClient.LeaveMulticastGroup(string)"></a>
### void LeaveMulticastGroup(string groupIp)

[`src/libraries/z42.net/src/UdpClient.z42:172`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L172)

离开 IPv4 组播组，接口交给 OS（0.0.0.0）。

<a id="Std.Net.Sockets.UdpClient.SetMulticastLoop(bool)"></a>
### void SetMulticastLoop(bool enable)

[`src/libraries/z42.net/src/UdpClient.z42:177`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L177)

设置 IP_MULTICAST_LOOP（本 socket 发出的组播是否回环到本机）。

<a id="Std.Net.Sockets.UdpClient.SetTtl(int)"></a>
### void SetTtl(int ttl)

[`src/libraries/z42.net/src/UdpClient.z42:186`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L186)

设置单播发送的 IP_TTL。

<a id="Std.Net.Sockets.UdpClient.SetReadTimeout(int)"></a>
### void SetReadTimeout(int millis)

[`src/libraries/z42.net/src/UdpClient.z42:195`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L195)

设置读超时（毫秒）；`millis <= 0` 清除超时。需先 Bind。

<a id="Std.Net.Sockets.UdpClient.SetWriteTimeout(int)"></a>
### void SetWriteTimeout(int millis)

[`src/libraries/z42.net/src/UdpClient.z42:204`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L204)

设置写超时（毫秒）；`millis <= 0` 清除超时。需先 Bind。

<a id="Std.Net.Sockets.UdpClient.LocalPort()"></a>
### int LocalPort()

[`src/libraries/z42.net/src/UdpClient.z42:213`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L213)

本地绑定端口（用 port 0 绑定时查询 OS 分配到的端口）。

<a id="Std.Net.Sockets.UdpClient.BindHost()"></a>
### string BindHost()

[`src/libraries/z42.net/src/UdpClient.z42:224`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L224)

返回绑定主机。

<a id="Std.Net.Sockets.UdpClient.Dispose()"></a>
### void Dispose()

[`src/libraries/z42.net/src/UdpClient.z42:229`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L229)

关闭 socket；幂等。之后的操作抛 `SocketClosedException`。

<a id="Std.Net.Sockets.UdpClient.Close()"></a>
### void Close()

[`src/libraries/z42.net/src/UdpClient.z42:239`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpClient.z42#L239)

`Dispose()` 的别名，对齐 .NET 的 `UdpClient.Close()`。

## UdpReceiveResult

[`src/libraries/z42.net/src/UdpReceiveResult.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpReceiveResult.z42#L5)

`UdpClient.Receive()` 的返回载体：数据报字节 + 发送方 IP + 发送方端口。

```z42
public class UdpReceiveResult {
    byte[] Buffer
    string RemoteHost
    int RemotePort
    UdpReceiveResult(byte[] buffer, string remoteHost, int remotePort)
}
```

成员：[`Buffer`](#Std.Net.Sockets.UdpReceiveResult.Buffer) · [`RemoteHost`](#Std.Net.Sockets.UdpReceiveResult.RemoteHost) · [`RemotePort`](#Std.Net.Sockets.UdpReceiveResult.RemotePort) · [`UdpReceiveResult`](#Std.Net.Sockets.UdpReceiveResult.ctor(byte[],string,int))

<a id="Std.Net.Sockets.UdpReceiveResult.Buffer"></a>
### byte[] Buffer

[`src/libraries/z42.net/src/UdpReceiveResult.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpReceiveResult.z42#L7)

数据报字节。

<a id="Std.Net.Sockets.UdpReceiveResult.RemoteHost"></a>
### string RemoteHost

[`src/libraries/z42.net/src/UdpReceiveResult.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpReceiveResult.z42#L9)

发送方主机。

<a id="Std.Net.Sockets.UdpReceiveResult.RemotePort"></a>
### int RemotePort

[`src/libraries/z42.net/src/UdpReceiveResult.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpReceiveResult.z42#L11)

发送方端口。

<a id="Std.Net.Sockets.UdpReceiveResult.ctor(byte[],string,int)"></a>
### UdpReceiveResult(byte[] buffer, string remoteHost, int remotePort)

[`src/libraries/z42.net/src/UdpReceiveResult.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.net/src/UdpReceiveResult.z42#L14)

以数据报字节与发送方地址组装。

