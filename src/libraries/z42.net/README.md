# z42.net — 网络库

## 职责

同步阻塞的网络类型：TCP / UDP / DNS / TLS 客户端、HTTP/1.1 客户端与服务端、WebSocket 客户端与服务端。
HTTP 与 WebSocket 为纯脚本，构建在 `TcpClient` / `TcpListener`（https 经 `TlsClient`）之上。
不含 async I/O。

## 功能索引

| 命名空间 | 公开类型 |
|---------|---------|
| `Std.Net.Sockets` | `TcpClient` / `TcpListener` / `NetworkStream`、`UdpClient` / `UdpReceiveResult`、`IPAddress` / `IPEndPoint`、`Dns`、`TlsClient` / `TlsStream` |
| `Std.Net.Http` | `HttpClient` / `HttpRequest` / `HttpResponse` / `HttpHeaders` / `HttpMethod` / `HttpStatusCode` / `HttpUrl`、`Cookie` / `CookieJar`、`HttpServer` / `HttpServerContext` |
| `Std.Net.WebSockets` | `WebSocketClient` / `WebSocketServer` / `WebSocketConnection` / `WebSocketMessage` / `WebSocketMessageType` / `WebSocketState` |
| `Std` | `NetException` / `NetUnsupportedException` / `SocketException` / `SocketClosedException` / `HttpException` / `HttpProtocolException` / `WebSocketException` / `WebSocketProtocolException` |

API 参考：[`docs/reference/src/stdlib/net.md`](../../../docs/reference/src/stdlib/net.md)。

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `src/TcpClient.z42` | `TcpClient` | TCP 客户端（`Connect` / `GetStream` / 读写超时 / `Close`） |
| `src/TcpListener.z42` | `TcpListener` | TCP 服务端（`Create` / `Bind` / `Start` / `AcceptTcpClient` / `Stop`） |
| `src/NetworkStream.z42` | `NetworkStream` | `Std.IO.Stream` 子类，字节经 socket 读写 |
| `src/UdpClient.z42` / `src/UdpReceiveResult.z42` | `UdpClient` / `UdpReceiveResult` | UDP socket（首次 Send 自动 bind）/ `Receive()` 结果 `{ Buffer, RemoteHost, RemotePort }` |
| `src/IPAddress.z42` / `src/IPEndPoint.z42` | `IPAddress` / `IPEndPoint` | v4 / v6 地址与端点值类型（`Parse` / `ToString`） |
| `src/Dns.z42` | `static class Dns` | 同步 DNS 解析 |
| `src/TlsClient.z42` | `TlsClient` / `TlsStream` | TLS 客户端与流（`TlsSocketNative` 的封装） |
| `src/SocketError.z42` | `enum SocketError` | `SocketException.SocketErrorCode` 的取值（.NET 同名枚举，Winsock 码） |
| `src/Exceptions/` | 异常 | `NetException` → `SocketException`（`SocketErrorCode` / `ErrorCode`）；`SocketClosedException : ObjectDisposedException`；`NetUnsupportedException : PlatformNotSupportedException`（wasm32） |
| `src/Security/AuthenticationException.z42` | 异常 | `Std.Security.Authentication.AuthenticationException`：TLS 握手 / 证书失败 |
| `src/Http/HttpClient.z42` | `HttpClient` | Get / Post / PostString / Send / SendStreaming；超时、重定向、cookie jar、自动解压；http 与 https |
| `src/Http/HttpRequest.z42` / `HttpResponse.z42` / `HttpHeaders.z42` | 请求 / 响应 / 大小写不敏感 header | builder 风格请求；`IsSuccess()` / `BodyAsString()` |
| `src/Http/HttpMethod.z42` / `HttpStatusCode.z42` / `HttpUrl.z42` | 常量 / URL 解析 | method 与常见状态码常量；scheme / host / port / path / query 解析 |
| `src/Http/Cookie.z42` / `CookieJar.z42` | `Cookie` / `CookieJar` | RFC 6265 子集的 cookie 记录、存储与匹配 |
| `src/Http/HttpServer.z42` / `HttpServerContext.z42` | `HttpServer` / `HttpServerContext` | HTTP/1.1 服务端（路由、线程池 / thread-per-accept）与 per-request 上下文 |
| `src/Http/_HttpRequestParser.z42` / `_HttpBodyStream.z42` | internal | 服务端请求解析；流式 body 的 framing 感知 Stream |
| `src/Http/HttpException.z42` / `HttpProtocolException.z42` | 异常 | HTTP 基类异常 / wire format 违规 |
| `src/WebSockets/WebSocketClient.z42` / `WebSocketServer.z42` / `WebSocketConnection.z42` | RFC 6455 客户端 / 服务端 / 连接句柄 | Connect·Accept / SendText / SendBinary / SendPing / Receive / Close |
| `src/WebSockets/WebSocketMessage.z42` / `WebSocketMessageType.z42` / `WebSocketState.z42` | 消息 / opcode 常量 / 状态常量 | `IsText` / `IsBinary` / `IsClose` / `AsString` 等 |
| `src/WebSockets/_FrameCodec.z42` | internal | RFC 6455 §5 帧编解码 |
| `src/WebSockets/WebSocketException.z42` / `WebSocketProtocolException.z42` | 异常 | WS 基类异常 / 协议违规 |

## 如何测试验证

```bash
xtask test stdlib z42.net    # 本库全部 [Test]（本机 loopback，无外网依赖）
```

## 关联文档

- 用法与规则（参考手册）：[z42.net](../../../docs/reference/src/stdlib/net.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)

## 依赖关系

- `z42.core`：基础类型 / 异常基类
- `z42.io`：`Std.IO.Stream`（`NetworkStream` 继承）
- `z42.encoding` / `z42.random` / `z42.crypto`：WebSocket 握手（Base64 key、frame mask、Sha1 accept 校验）
- `z42.threading`：`HttpServer` 的 thread-per-accept
- `z42.compression`：`HttpClient` 自动解压（Gzip / Brotli）

native 后端：socket op 走 z42.core 的 `TcpSocketNative` / `UdpSocketNative` / `TlsSocketNative` /
`DnsNative.Resolve`（`__tcp_*` / `__udp_*` / `__tls_*` / `__dns_lookup`，builtin ABI v2：返回普通值、失败从 Rust 抛），
VM 侧实现在 `src/runtime/src/corelib/network.rs`（+ `network/`）与 `tls.rs`，slot table 模式镜像 `Std.IO.ProcessHandle`。
`NetworkStream` 把 `SocketException` 包成 `IOException`（`InnerException` 是原异常），同 .NET；`TlsStream` 的读写失败由 VM 直接抛 `IOException`。
