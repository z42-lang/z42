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

同步阻塞的 DNS 查询（getaddrinfo）。

```z42
public static class Dns {
    static IPAddress[] GetHostAddresses(string host)
}
```

### static IPAddress[] GetHostAddresses(string host)

把主机名解析为 IPAddress 数组；解析失败抛 `SocketException`。 数字字面量（如 "127.0.0.1" / "::1"）原样返回，不走网络。

## SocketClosedException

在已 `Dispose` / `Close` / `Stop` 的 socket 或 listener 上操作时抛出（含阻塞中的 accept / 读写 被另一线程关掉）；继承 `ObjectDisposedException`。

```z42
public class SocketClosedException : ObjectDisposedException {
    SocketClosedException()
}
```

### SocketClosedException()


## SocketException

socket 操作（connect / bind / accept / send / receive / 选项 / DNS）因系统错误失败时抛出。 超时也是它（`SocketErrorCode == SocketError.TimedOut`），与 .NET 的 `Socket.Receive` 一致。

```z42
public class SocketException : NetException {
    SocketError SocketErrorCode
    int ErrorCode
    SocketException(string message)
    SocketException(int errorCode, string message)
}
```

### SocketError SocketErrorCode

结构化错误码（`SocketError`，Winsock 取值）。

### int ErrorCode

与 `SocketErrorCode` 同一个值的 int。

### SocketException(string message)


### SocketException(int errorCode, string message)


## IPAddress

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

### static int FamilyIPv4

IPv4 的 family 常量值。

### static int FamilyIPv6

IPv6 的 family 常量值。

### IPAddress(int family, byte[] bytes)

以 family（4 或 6）与字节数组构造；family 4 需 4 字节、6 需 16 字节。

### static IPAddress Loopback()

IPv4 回环地址（127.0.0.1）。

### static IPAddress Any()

IPv4 任意地址（0.0.0.0）。

### static IPAddress IPv6Loopback()

IPv6 回环地址（::1）。

### static IPAddress IPv6Any()

IPv6 任意地址（::）。

### static IPAddress Parse(string s)

把字符串解析为 IPAddress；失败抛 `FormatException`。

### static IPAddress? TryParse(string s)

非抛异常解析；不是合法的 IPv4 / IPv6 字面量时返回 null。

### int Family()

返回地址族（4 或 6）。

### bool IsIPv4()

是否 IPv4 地址。

### bool IsIPv6()

是否 IPv6 地址。

### byte[] GetAddressBytes()

返回原始字节的副本（IPv4 为 4 字节、IPv6 为 16 字节）。

### bool IsLoopback()

是否回环地址（IPv4 的 127.0.0.0/8 或 IPv6 的 ::1）。

### bool IsAny()

是否任意地址（IPv4 的 0.0.0.0 或 IPv6 的 ::）。

### bool IsMulticast()

是否组播地址（IPv4 的 224.0.0.0/4 或 IPv6 的 ff00::/8）。

### override string ToString()

规范字符串；IPv4 用点分十进制，IPv6 用最长零段折叠（小写 hex）。

### bool Equals(IPAddress other)

与另一个 IPAddress 比较是否相等。

## IPEndPoint

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

### static int MinPort

最小端口值。

### static int MaxPort

最大端口值。

### IPEndPoint(IPAddress address, int port)

以地址和端口构造；port 越界抛 `ArgumentException`。

### IPAddress Address()

返回地址。

### int Port()

返回端口。

### override string ToString()

规范形式：IPv4 → `addr:port`；IPv6 → `[addr]:port`。

### static IPEndPoint Parse(string s)

解析 `addr:port`（IPv4）或 `[addr]:port`（IPv6）形式；失败抛 `FormatException`。

### bool Equals(IPEndPoint other)

与另一个 IPEndPoint 比较是否相等。

## NetworkStream

TCP socket 之上的字节流，`Std.IO.Stream` 的子类，由 `TcpClient.GetStream()` 给出。

```z42
public class NetworkStream : Stream {
    NetworkStream(long slotId)
    void MarkClosed()
}
```

### NetworkStream(long slotId)


### void MarkClosed()


## SocketError

socket 操作的错误码，镜像 .NET 的 `SocketError`，取值即 Winsock 码。 读 `SocketException.SocketErrorCode`。

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

### TcpClient()


### static TcpClient ConnectTo(string host, int port)

等价于 `new TcpClient()` 后再 `Connect(host, port)` 的静态工厂。

### void Connect(string host, int port)

建立到 `host:port` 的 TCP 连接，阻塞直到握手完成或失败。 连接被拒 / DNS 失败 / 网络不可达抛 `SocketException`。

### void SetConnectTimeout(int millis)

设置下一次 `Connect` 的超时（毫秒）；`millis <= 0` 清除超时。 只作用于紧随其后的一次 Connect；该值在同一实例上跨重连持续有效。

### NetworkStream GetStream()

返回该连接的字节流 `NetworkStream`；惰性创建并缓存，重复调用返回同一实例。

### string RemoteHost()

返回连接的远端主机。

### int RemotePort()

返回连接的远端端口。

### void SetReadTimeout(int millis)

设置读超时（毫秒）；`millis <= 0` 清除超时。

### void SetWriteTimeout(int millis)

设置写超时（毫秒）；`millis <= 0` 清除超时。

### void SetNoDelay(bool enable)

设置 TCP_NODELAY（禁用 Nagle 算法）。

### void SetTtl(int ttl)

设置 IP_TTL（跳数限制），`ttl` 通常在 [1, 255]。

### void SetKeepAlive(bool enable)

开启或关闭 SO_KEEPALIVE（使用系统默认的 idle / interval / probe 参数）。

### void SetKeepAlive(bool enable, int idleSecs, int intervalSecs, int probes)

带调参的 SO_KEEPALIVE；`enable == true` 时三个调参值都必须 >= 1。

### void Dispose()

关闭 socket 并释放底层 fd；幂等。之后的操作抛 `SocketClosedException`。

### void Close()

`Dispose()` 的别名，对齐 .NET 的 `TcpClient.Close()`。

### TcpClient(long slotId, bool fromAccept)


## TcpListener

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

### TcpListener()


### static TcpListener Create(string host, int port)

等价于 `new TcpListener()` 后 `Bind(host, port)` 的静态工厂。

### void Bind(string host, int port)

绑定到 `host:port` 并开始监听；port 0 表示由 OS 分配端口（事后用 `LocalPort()` 查询）。

### void SetReuseAddress(bool enable)

设置 SO_REUSEADDR（只能在 `Bind` 之前调用）；Bind 之后再调抛 `InvalidOperationException`。

### void Start()

BCL 兼容的空操作；`Bind` 已经原子地完成了 bind + listen。

### TcpClient AcceptTcpClient()

阻塞直到有对端连接，返回包装已接受 socket 的 `TcpClient`。

### int LocalPort()

本地绑定端口 —— 用 port 0 绑定时查询 OS 分配到的端口。

### string BindHost()

返回绑定主机。

### void SetTtl(int ttl)

设置监听 socket 的 IP_TTL（Bind 之后调用）。

### void Stop()

停止接受并释放监听 socket；幂等。

### void Dispose()

释放监听 socket；幂等。

## TlsStream

TLS 会话之上的字节流（rustls 客户端侧），`NetworkStream` 的 TLS 版。

```z42
public class TlsStream : Stream {
    TlsStream(long slotId)
    void MarkClosed()
}
```

### TlsStream(long slotId)


### void MarkClosed()


## TlsClient

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

### TlsClient()


### void Connect(string host, int port, int timeoutMs)

建立到 `host:port` 的 TCP 连接并完成 TLS 握手；`timeoutMs > 0` 同时约束两者。

### Stream GetStream()

返回该会话的字节流（具体类型为 `TlsStream`）；惰性创建并缓存。

### string RemoteHost()

返回连接的远端主机。

### int RemotePort()

返回连接的远端端口。

### void SetReadTimeout(int millis)

设置读超时（毫秒）；`millis <= 0` 清除超时。

### void SetWriteTimeout(int millis)

设置写超时（毫秒）；`millis <= 0` 清除超时。

### void Dispose()

关闭会话并释放底层 fd；幂等。

### void Close()

`Dispose()` 的别名，对齐 `TcpClient.Close()`。

## UdpClient

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

### UdpClient()


### void Bind(string host, int port)

绑定到 `host:port`；port 0 由 OS 分配临时端口（事后用 `LocalPort()` 查询）。

### int Send(byte[] data, int length, string remoteHost, int remotePort)

向 `remoteHost:remotePort` 发送 `data[0..length]` 的数据报，返回发送字节数。 若尚未 Bind，则自动绑定到 ("0.0.0.0", 0)。

### UdpReceiveResult Receive()

阻塞直到收到一个数据报，返回数据报及其发送方地址。

### void Connect(string remoteHost, int remotePort)

设置默认远端地址，供后续 `SendConnected` 使用；不调用 OS 的 connect(2)。

### void Disconnect()

清除默认远端地址；幂等。

### bool IsConnected()

是否已 Connect 且未 Disconnect。

### int SendConnected(byte[] data, int length)

向 `Connect` 设置的远端发送 `data[0..length]`；未 Connect 抛 `InvalidOperationException`。

### int SendConnected(byte[] data)

向已连接的远端发送整个 `data` 数组。

### int ReceiveInto(byte[] buf, int offset, int count)

把数据报收进调用方缓冲 `buf[offset..offset+count)`，省一次分配；返回写入字节数。 对端地址通过 `LastReceiveHost` / `LastReceivePort` 读取。

### int ReceiveIntoFull(byte[] buf)

把数据报收满整个 `buf`（offset 0、count buf.Length）。

### string LastReceiveHost()

最近一次 `ReceiveInto*` 的对端主机（成功接收前为空）。

### int LastReceivePort()

最近一次 `ReceiveInto*` 的对端端口（成功接收前为 0）。

### void JoinMulticastGroup(string groupIp, string ifaceIp)

加入 IPv4 组播组；`ifaceIp` 传 "0.0.0.0" 表示由 OS 选择接口。

### void JoinMulticastGroup(string groupIp)

加入 IPv4 组播组，接口交给 OS（0.0.0.0）。

### void LeaveMulticastGroup(string groupIp, string ifaceIp)

离开之前加入的 IPv4 组播组。

### void LeaveMulticastGroup(string groupIp)

离开 IPv4 组播组，接口交给 OS（0.0.0.0）。

### void SetMulticastLoop(bool enable)

设置 IP_MULTICAST_LOOP（本 socket 发出的组播是否回环到本机）。

### void SetTtl(int ttl)

设置单播发送的 IP_TTL。

### void SetReadTimeout(int millis)

设置读超时（毫秒）；`millis <= 0` 清除超时。需先 Bind。

### void SetWriteTimeout(int millis)

设置写超时（毫秒）；`millis <= 0` 清除超时。需先 Bind。

### int LocalPort()

本地绑定端口（用 port 0 绑定时查询 OS 分配到的端口）。

### string BindHost()

返回绑定主机。

### void Dispose()

关闭 socket；幂等。之后的操作抛 `SocketClosedException`。

### void Close()

`Dispose()` 的别名，对齐 .NET 的 `UdpClient.Close()`。

## UdpReceiveResult

`UdpClient.Receive()` 的返回载体：数据报字节 + 发送方 IP + 发送方端口。

```z42
public class UdpReceiveResult {
    byte[] Buffer
    string RemoteHost
    int RemotePort
    UdpReceiveResult(byte[] buffer, string remoteHost, int remotePort)
}
```

### byte[] Buffer

数据报字节。

### string RemoteHost

发送方主机。

### int RemotePort

发送方端口。

### UdpReceiveResult(byte[] buffer, string remoteHost, int remotePort)


