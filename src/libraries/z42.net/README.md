# z42.net — 网络库

## 职责

z42 标准网络类型。K1: TCP sockets only (sync blocking)。UDP / IPAddress / DNS / TLS / HTTP 走独立 follow-up spec。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `TcpClient.z42` | `TcpClient` | sync blocking TCP client (`Connect` / `GetStream` / `SetReadTimeout(ms)` / `SetWriteTimeout(ms)` / `Close` / `Dispose`) |
| `TcpListener.z42` | `TcpListener` | sync blocking TCP server (`Create(host, port)` static factory / `Bind` / `LocalPort` / `Start` / `AcceptTcpClient` / `Stop` / `Dispose`) |
| `NetworkStream.z42` | `NetworkStream` | extends `Std.IO.Stream`；read/write 字节经由 socket fd |
| `UdpClient.z42` | `UdpClient` | sync blocking UDP socket (`Bind` / `Send` / `Receive` / `LocalPort` / `Close` / `Dispose`); auto-bind on first Send |
| `UdpReceiveResult.z42` | `UdpReceiveResult` | `{ Buffer, RemoteHost, RemotePort }` carrier returned by `UdpClient.Receive()` |
| `IPAddress.z42` | `IPAddress` | strongly-typed v4/v6 address: `Parse(string)` / `ToString` round-trip / `Loopback/Any/IPv6Loopback/IPv6Any` factories / `IsLoopback/IsAny/IsMulticast` / `GetAddressBytes` / `Equals` |
| `IPEndPoint.z42` | `IPEndPoint` | `(IPAddress, port)` value: ctor validates port ∈ [0, 65535]; `Address() / Port()` accessors; `ToString` emits `addr:port` (IPv4) or `[addr]:port` (IPv6); `Parse(string)` round-trips both forms; rejects unbracketed IPv6 as ambiguous |
| `Exceptions/NetException.z42` | `NetException` | z42.net 异常基类 |
| `Exceptions/NetUnsupportedException.z42` | `NetUnsupportedException` | wasm32 / 未支持平台抛出 |
| `Exceptions/SocketException.z42` | `SocketException` | 连接 / 读写失败 |
| `Exceptions/SocketClosedException.z42` | `SocketClosedException` | 已关闭 socket 上的操作 |

## 入口点

- `Std.Net.Sockets.TcpClient` / `TcpListener` / `NetworkStream` (TCP)
- `Std.Net.Sockets.UdpClient` / `UdpReceiveResult` (UDP)
- `Std.Net.Http.HttpClient` / `HttpRequest` / `HttpResponse` / `HttpHeaders` /
  `HttpMethod` / `HttpStatusCode` / `HttpUrl` (HTTP/1.1)
- `Std.Net.WebSockets.WebSocketClient` / `WebSocketMessage` /
  `WebSocketMessageType` / `WebSocketState` (WebSocket ws://)

## src/WebSockets/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `WebSocketClient.z42` | `WebSocketClient` | RFC 6455 client: Connect / SendText / SendBinary / SendPing / Receive / Close / Dispose. Pure-script over K1 `TcpClient` |
| `WebSocketMessage.z42` | `WebSocketMessage` | `{ MessageType, Buffer }` + `IsText/IsBinary/IsClose` + `AsString` / `CloseStatus` / `CloseReason` |
| `WebSocketMessageType.z42` | `WebSocketMessageType` | int constants matching RFC 6455 opcodes (Text=1, Binary=2, Close=8, Ping=9, Pong=10) |
| `WebSocketState.z42` | `WebSocketState` | lifecycle state constants (Connecting / Open / CloseSent / Closed) |
| `_FrameCodec.z42` | `_FrameCodec` | internal RFC 6455 §5 frame encode/decode |
| `WebSockets/WebSocketException.z42` | `WebSocketException` | base WS exception |
| `WebSockets/WebSocketProtocolException.z42` | `WebSocketProtocolException` | RFC violations |

## src/Http/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `HttpClient.z42` | `HttpClient` | top-level facade: Get / Post / PostString / Send + `SetTimeout(ms)` 读写超时 + `SetMaxRedirects` + `SetCookieJar`. Pure-script over K1 `TcpClient`. http:// only — https:// throws `NotSupportedException` pending `add-z42-net-tls` |
| `HttpRequest.z42` | `HttpRequest` | method + url + headers + body 容器 (builder pattern via `SetHeader` / `SetBody`) |
| `HttpResponse.z42` | `HttpResponse` | parsed status + reason + headers + body; `IsSuccess()` / `BodyAsString()` |
| `HttpHeaders.z42` | `HttpHeaders` | case-insensitive header dict (raw `string[]`+count; z42 不支持 generic field types) |
| `HttpMethod.z42` | `HttpMethod` | wire-format method string constants (Get/Post/Put/Delete/Patch/Head/Options) |
| `HttpStatusCode.z42` | `HttpStatusCode` | common int constants (Ok=200, NotFound=404, ...) |
| `HttpUrl.z42` | `HttpUrl` | minimal URL parser (scheme / host / port / path / query); http only |
| `Http/HttpException.z42` | `HttpException` | base HTTP exception (namespace Std) |
| `Http/HttpProtocolException.z42` | `HttpProtocolException` | wire format violations |

## 依赖关系

- `z42.core` — 基础类型 / 异常基类
- `z42.io` — `Std.IO.Stream` base class（`NetworkStream` 继承）

## Native 后端

所有 socket op 走 `__net_tcp_*` / `__net_udp_*` builtin，VM-side 实现位于
`src/runtime/src/corelib/network.rs`，slot tables 在
`VmContext.tcp_sockets` / `tcp_listeners` / `udp_sockets`，pattern 镜像
`Std.IO.ProcessHandle`。
