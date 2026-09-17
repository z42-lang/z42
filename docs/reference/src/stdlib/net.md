# z42.net —— TCP / UDP / TLS / HTTP / WebSocket

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.net/`；命名空间 `Std.Net.Sockets` / `Std.Net.Http` /
> `Std.Net.WebSockets`（异常类在 `Std`）

网络栈：原始 socket（TCP / UDP）、TLS 客户端、HTTP/1.1 客户端与服务端、
WebSocket 客户端与服务端、DNS 解析、`IPAddress` / `IPEndPoint`。

**全部是同步阻塞 API**——没有 async/await，没有非阻塞 socket，没有 selector。并发靠
`Std.Threading.Thread`（`HttpServer` 自带线程化与线程池两个入口）。超时以
`SetReadTimeout` / `SetWriteTimeout` / `SetConnectTimeout` 这类**毫秒级 setter** 表达，
不是每次调用的参数；`millis <= 0` 表示清除超时（回到无限阻塞）。

wasm32 目标上所有 socket 操作抛 `NetUnsupportedException`。

---

## 异常

全部在 `Std` 命名空间：

```
Exception
└── NetException
    ├── NetUnsupportedException     // 目标平台（wasm32）无 socket
    ├── SocketException             // 连接失败 / IO 失败 / 超时 / DNS 失败
    ├── SocketClosedException       // 对已关闭或已 Dispose 的句柄再操作
    ├── HttpException
    │   └── HttpProtocolException   // 响应不符合 HTTP/1.1 语法
    └── WebSocketException
        └── WebSocketProtocolException
```

`SocketException` 的 message 带 OS 错误原文，例如
`connect to 127.0.0.1:1 (timeout 300ms): Connection refused (os error 61)`。
读超时表现为 `SocketException: udp recv: Resource temporarily unavailable (os error 35)`
一类的消息，没有独立的 timeout 异常类型。

解析类错误（URL、IP 字面量）走 `Std.FormatException`；参数越界走
`Std.ArgumentException`；生命周期用错（未 bind 就用、bind 后再设 pre-bind 选项）走
`Std.InvalidOperationException`。

---

## Std.Net.Sockets

### IPAddress

```z42
public static int FamilyIPv4;      // 4
public static int FamilyIPv6;      // 6

public IPAddress(int family, byte[] bytes)      // family 4 → 4 字节；6 → 16 字节

public static IPAddress  Loopback()             // 127.0.0.1
public static IPAddress  Any()                  // 0.0.0.0
public static IPAddress  IPv6Loopback()         // ::1
public static IPAddress  IPv6Any()              // ::
public static IPAddress  Parse(string s)        // 失败抛 FormatException
public static IPAddress? TryParse(string s)     // 失败返回 null

public int    Family()
public bool   IsIPv4()
public bool   IsIPv6()
public byte[] GetAddressBytes()                 // 返回副本
public bool   IsLoopback()                      // 127.0.0.0/8 或 ::1
public bool   IsAny()
public bool   IsMulticast()                     // 224.0.0.0/4 或 ff00::/8
public override string ToString()
public bool   Equals(IPAddress other)
```

**IPv6 支持的输入形态**（全部实测通过）：

| 形态 | 例子 |
|---|---|
| 完整 8 组 | `2001:0db8:0000:0000:0000:0000:0000:0001` |
| `::` 省略一段零 | `::1` / `::` / `2001:db8::1` |
| IPv4-mapped | `::ffff:192.0.2.1` |
| IPv4-compatible | `::192.0.2.1` |
| 带前缀的 dotted 尾（NAT64） | `64:ff9b::1.2.3.4` |

`ToString()` 一律输出规范 hex 形式并做最长零段折叠——`::ffff:192.0.2.1` 回显为
`::ffff:c000:201`，`64:ff9b::1.2.3.4` 回显为 `64:ff9b::102:304`。**不保证字符串往返**，
只保证 `Parse(ToString(x)).Equals(x)`。

**不支持**：zone id 后缀（`fe80::1%en0` 抛 `FormatException`）、CIDR 前缀长度、
子网掩码运算、`IsLinkLocal` / `IsPrivate` 之类的分类谓词。

### IPEndPoint

```z42
public static int MinPort;    // 0
public static int MaxPort;    // 65535

public IPEndPoint(IPAddress address, int port)   // port 越界抛 ArgumentException
public IPAddress Address()
public int       Port()
public override string ToString()                // IPv4 → "a.b.c.d:p"；IPv6 → "[addr]:p"
public static IPEndPoint Parse(string s)
public bool Equals(IPEndPoint other)
```

`Parse` 两种形态都收；**不带方括号的 IPv6** 被判为歧义并抛 `FormatException`
（`"::1:80"` → `unbracketed IPv6 is ambiguous`）。

注意：`IPEndPoint` 目前只是值对象，**socket API 不接受它**——`Connect` / `Bind` / `Send`
一律是 `(string host, int port)` 两个参数。

### Dns

```z42
public static class Dns {
    public static IPAddress[] GetHostAddresses(string host)
}
```

阻塞式 `getaddrinfo`。数字字面量（`"127.0.0.1"` / `"::1"`）原样回来，不走网络。
双栈主机上 IPv4 与 IPv6 混排，顺序由 OS 解析器决定（实测 `localhost` 先返回 `::1`），
要挑族自己用 `IsIPv4()` / `IsIPv6()` 过滤。解析失败抛 `SocketException`。

**不支持**：SRV / MX / TXT / PTR 记录、反查、每次查询的超时（走 OS 默认）、取消。

### TcpClient

```z42
public TcpClient()
public static TcpClient ConnectTo(string host, int port)

public void Connect(string host, int port)
public NetworkStream GetStream()                 // 惰性创建并缓存
public string RemoteHost()
public int    RemotePort()

public void SetConnectTimeout(int millis)        // 作用于下一次 Connect
public void SetReadTimeout(int millis)           // Connect 之后调用
public void SetWriteTimeout(int millis)
public void SetNoDelay(bool enable)              // TCP_NODELAY
public void SetTtl(int ttl)                      // IP_TTL
public void SetKeepAlive(bool enable)            // SO_KEEPALIVE
public void SetKeepAlive(bool enable, int idleSecs, int intervalSecs, int probes)

public void Dispose()
public void Close()                              // Dispose 的别名
```

- `host` 走 DNS 解析，直接传 IP 字面量也行。
- `Dispose` / `Close` 幂等；之后再 `GetStream()` 抛 `SocketClosedException`。
- keepalive 调参的生效范围依赖 OS：idle 与 interval 在 Unix / Windows 都生效，
  probes 只在 Linux / Android / FreeBSD 生效；传 0 或负值抛 `SocketException`。
- **没有** `SetReuseAddress`（那是 `TcpListener` 侧的能力）。

### TcpListener

```z42
public TcpListener()
public static TcpListener Create(string host, int port)

public void Bind(string host, int port)          // port 0 → OS 分配
public void SetReuseAddress(bool enable)         // 只能在 Bind 之前调用
public void Start()                              // no-op，为习惯保留
public TcpClient AcceptTcpClient()               // 阻塞
public int    LocalPort()
public string BindHost()
public void SetTtl(int ttl)                      // Bind 之后
public void Stop()                               // Dispose 的别名
public void Dispose()
```

`SetReuseAddress` 在 `Bind` 之后调用抛
`InvalidOperationException: SetReuseAddress: TcpListener already bound — call before Bind()`。

监听 socket 没有 accept 超时，也没有非阻塞 accept——想中断阻塞中的 `AcceptTcpClient`，
只能从另一个线程 `Stop()`，被打断的 accept 会抛 `SocketException` / `SocketClosedException`。

### NetworkStream

`Std.IO.Stream` 的子类，由 `TcpClient.GetStream()` 给出。

```z42
override bool CanRead()      // true
override bool CanWrite()     // true
override bool CanSeek()      // 恒 false
override int  Read(byte[] buffer, int offset, int count)   // 返回读到的字节数；0 = 对端关闭
override void Write(byte[] buffer, int offset, int count)
override void Close()
```

`Read` 是**部分读**——返回值可能小于 `count`，需要循环读满。

### TlsClient / TlsStream

客户端侧 TLS。证书校验**始终开启**，信任根是内置的 Mozilla 根证书集；握手或证书校验
失败抛 `SocketException`，没有降级到明文的路径。

```z42
public class TlsClient {
    public TlsClient()
    public void   Connect(string host, int port, int timeoutMs)   // TCP 连接 + 握手一并完成
    public Stream GetStream()                                     // TlsStream : Stream
    public string RemoteHost()
    public int    RemotePort()
    public void   SetReadTimeout(int millis)
    public void   SetWriteTimeout(int millis)
    public void   Dispose()
    public void   Close()
}
```

`Connect` 的 `timeoutMs` 同时约束 TCP 连接与 TLS 握手；`> 0` 才生效。握手在 `Connect`
里完成，证书 / 协议错误立刻暴露，不会拖到第一次 `Read`。

**不支持**：服务端 TLS（没有 `TlsListener`）、客户端证书、系统 / 企业信任库、
自定义校验回调、TLS 连接池。

### UdpClient

```z42
public UdpClient()
public void Bind(string host, int port)          // port 0 → OS 分配

// 无连接收发
public int Send(byte[] data, int length, string remoteHost, int remotePort)  // 首次调用会自动 bind
public UdpReceiveResult Receive()                                            // 阻塞

// 连接模式
public void Connect(string remoteHost, int remotePort)
public void Disconnect()
public bool IsConnected()
public int  SendConnected(byte[] data, int length)
public int  SendConnected(byte[] data)

// 收进调用方缓冲（省一次分配）
public int    ReceiveInto(byte[] buf, int offset, int count)
public int    ReceiveIntoFull(byte[] buf)
public string LastReceiveHost()                  // 最近一次 ReceiveInto* 的对端
public int    LastReceivePort()

// 组播
public void JoinMulticastGroup(string groupIp)
public void JoinMulticastGroup(string groupIp, string ifaceIp)
public void LeaveMulticastGroup(string groupIp)
public void LeaveMulticastGroup(string groupIp, string ifaceIp)
public void SetMulticastLoop(bool enable)

public void SetTtl(int ttl)
public void SetReadTimeout(int millis)
public void SetWriteTimeout(int millis)
public int    LocalPort()
public string BindHost()
public void Dispose()
public void Close()
```

`Receive()` 每次返回一整个数据报；读超时到点抛 `SocketException`。
`ReceiveInto*` 的对端信息不在返回值里，要紧接着读 `LastReceiveHost()` / `LastReceivePort()`。

```z42
public class UdpReceiveResult {
    public byte[] Buffer;
    public string RemoteHost;
    public int    RemotePort;
    public UdpReceiveResult(byte[] buffer, string remoteHost, int remotePort)
}
```

---

## Std.Net.Http

### HttpClient

```z42
public HttpClient()

public HttpResponse Get(string url)
public HttpResponse Post(string url, byte[] body, string contentType)
public HttpResponse PostString(string url, string body, string contentType)
public HttpResponse Send(HttpRequest request)
public HttpResponse SendStreaming(HttpRequest request)   // 响应体走 BodyStream，不缓冲

public void SetTimeout(int millis);         public int  GetTimeout()
public void SetMaxRedirects(int n);         public int  GetMaxRedirects()
public void SetKeepAlive(bool enable);      public bool GetKeepAlive()
public void SetAutoDecompress(bool enable); public bool GetAutoDecompress()
public void SetCookieJar(CookieJar jar);    public CookieJar GetCookieJar()
public void SetUserAgent(string userAgent)
public void Dispose()                       // 释放连接池；之后仍可继续用
```

**默认值全部是"关"**：`timeout = 0`（不超时）、`maxRedirects = 0`（不跟随）、
`keepAlive = false`、`autoDecompress = false`、`cookieJar = null`、
`User-Agent = "z42-http/0.1"`。

| 开关 | 打开后 |
|---|---|
| `SetTimeout` | 读写超时（毫秒）。`http://` 下**不覆盖连接阶段**——TCP 连接超时走 OS 默认（macOS 75s / Linux ~127s）；`https://` 下同一个值还会约束 TLS 连接 + 握手 |
| `SetMaxRedirects` | 跟随 301 / 302 / 303（转 GET）与 307 / 308（保留方法与 body）；超过跳数抛 `HttpException`。304 及其它状态码不跟随 |
| `SetKeepAlive` | 发 `Connection: keep-alive`，按 `host:port` 缓存连接（上限 8 条，超出淘汰最旧）；对陈旧连接失败会静默重试一次 |
| `SetAutoDecompress` | 发 `Accept-Encoding: gzip, br`，并把 gzip / brotli 响应体解开填进 `Body`（响应头原样保留） |
| `SetCookieJar` | 自动吃 `Set-Cookie`、自动带 `Cookie:` 头 |

发出去的请求会自动补这些头，**前提是调用方没有自己设过同名头**：`Host`、`User-Agent`、
`Connection`（keep-alive 开关决定值）、`Content-Length`（有 body 时）、
`Accept-Encoding`（开了自动解压时）、`Cookie`（挂了 jar 时）。想覆盖就在 `HttpRequest`
上先 `SetHeader`。

`SendStreaming` 只支持 `http://`；`https://` 抛 `NotSupportedException`
（提示改用 `Send` / `Get` 缓冲版本）。HTTPS 也不进 keep-alive 连接池，每次请求新开连接。

### HttpRequest

```z42
public string      Method;      // HttpMethod.Get / Post / ... 或自定义动词
public string      Url;         // 完整绝对 URL（客户端侧）
public HttpHeaders Headers;
public byte[]      Body;        // 无 body 时为 null
public string      DigestUser;
public string      DigestPassword;

public HttpRequest(string method, string url)

// 以下全部返回 this，可链式调用
public HttpRequest SetHeader(string name, string value)    // 覆盖同名
public HttpRequest AddHeader(string name, string value)    // 追加同名
public HttpRequest SetBody(byte[] body)
public HttpRequest WithBasicAuth(string user, string password)   // 立刻写 Authorization: Basic …
public HttpRequest WithBearerToken(string token)                 // 立刻写 Authorization: Bearer …
public HttpRequest WithDigestAuth(string user, string password)  // 只记凭据，不写头
```

`WithDigestAuth` 与前两个不同：它**不立即产生 `Authorization` 头**，而是把凭据存在
`DigestUser` / `DigestPassword` 上，等 `HttpClient` 收到 401 challenge 后自动重试时才
计算响应（RFC 2617 MD5 默认，`algorithm=SHA-256` 时走 RFC 7616，qop=auth）。

在服务端侧，`HttpServerContext.Request.Url` 装的是**请求行里的 target**（`"/path?q=1"`），
不是绝对 URL。

### HttpResponse

```z42
public int         StatusCode;
public string      ReasonPhrase;
public HttpHeaders Headers;
public byte[]      Body;          // SendStreaming 下为空数组
public Stream      BodyStream;    // 仅 SendStreaming 下非 null

public HttpResponse(int statusCode, string reasonPhrase, HttpHeaders headers, byte[] body)
public bool   IsSuccess()         // 2xx
public string BodyAsString()      // UTF-8 解码 Body
```

### HttpHeaders

大小写不敏感的多值头集合，保留插入顺序。

```z42
public HttpHeaders()
public void   Add(string name, string value)     // 追加，允许同名多条
public void   Set(string name, string value)     // 覆盖同名
public string Get(string name)                   // 首个值；**没有时返回 null**
public bool   Contains(string name)
public int    Remove(string name)                // 返回删掉的条数
public int    Count()                            // 条目总数（同名多条各算一条）
public string GetNameAt(int index)               // 原始大小写
public string GetValueAt(int index)
```

`Get` 返回 `null` 而不是空串——拼进字符串会得到字面 `"null"`，先 `Contains` 判一下。

### HttpMethod / HttpStatusCode

都是静态常量集，不是 enum。

```z42
HttpMethod.Get / Post / Put / Delete / Patch / Head / Options      // string
HttpStatusCode.Continue / SwitchingProtocols
              / Ok / Created / Accepted / NoContent / PartialContent
              / MovedPermanently / Found / SeeOther / NotModified
              / TemporaryRedirect / PermanentRedirect
              / BadRequest / Unauthorized / Forbidden / NotFound / MethodNotAllowed
              / Conflict / Gone / UnsupportedMediaType / TooManyRequests
              / InternalServerError / NotImplemented / BadGateway
              / ServiceUnavailable / GatewayTimeout                // int
```

### HttpUrl

```z42
public string Scheme;   // "http" 或 "https"
public string Host;
public int    Port;     // 显式端口，或 scheme 默认（80 / 443）
public string Path;     // 带前导 "/"；缺省为 "/"
public string Query;    // 带前导 "?"；无则空串

public HttpUrl(string scheme, string host, int port, string path, string query)
public string PathAndQuery()
public static HttpUrl Parse(string url)
```

只收 `http://` 与 `https://`，其它 scheme 抛 `FormatException`。
`user:pass@host` 形式抛 `NotSupportedException`。
**IPv6 字面量 host 不支持**——`http://[::1]:80/` 会因为方括号里的冒号被当成端口分隔符而
抛 `FormatException`。

### HttpServer / HttpServerContext

```z42
public class HttpServer {
    public HttpServer()
    public void Bind(string host, int port)        // port 0 → OS 分配
    public int    LocalPort()
    public string BindHost()

    public void HandleNext(Action<HttpServerContext> handler)     // 只处理一个请求
    public void Serve(Action<HttpServerContext> handler)          // 串行循环
    public void ServeThreaded(Action<HttpServerContext> handler)  // 每连接一线程
    public void ServeWithPool(Action<HttpServerContext> handler, int workerCount, int queueSize)

    public void Stop()
    public void Dispose()
}

public class HttpServerContext {
    public HttpRequest Request;
    public bool        ResponseSent;

    public void Send(HttpResponse response)
    public void SendBytes(int statusCode, string reasonPhrase, byte[] body, string contentType)
    public void SendText(int statusCode, string text)      // text/plain; charset=utf-8
    public void SendJson(int statusCode, string jsonBody)  // application/json; charset=utf-8
    public void SendStatus(int statusCode)                 // 空 body
}
```

- 每个响应后**连接一律关闭**——服务端不做 keep-alive；`Send` 会自动补上
  `Content-Length` 与 `Connection: close`（调用方没显式设的话）。
- 处理器抛异常且尚未发响应 → 自动回 500；请求行 / 头解析失败 → 自动回 400；
  处理器正常返回但一次 `Send*` 都没调 → 自动回 500。
- 重复 `Send*` 抛 `InvalidOperationException`。
- 没有路由表——路径分发自己在 handler 里按 `ctx.Request.Url` 判。
- 请求体在解析阶段一次读完，没有流式请求体。
- **没有 HTTPS 服务端**，也**不处理 WebSocket upgrade**（WebSocket 服务端是独立的
  `WebSocketServer`）。
- `HEAD` 请求不被特殊对待——`SendText` 之类照样写 body。

`Serve` 被另一个线程 `Stop()` 打断时正常返回；单次迭代内的其它异常会打到 stderr 后继续。

### Cookie / CookieJar

```z42
public class Cookie {
    public string Name; public string Value;
    public string Domain; public string Path;
    public bool Secure; public bool HttpOnly;
    public long ExpiresUnixSec;                 // 0 = 会话 cookie

    public Cookie(string name, string value)
    public bool IsExpired(long nowUnixSec)
    public bool Matches(string host, string path, bool isSecure)
}

public class CookieJar {
    public CookieJar()
    public void Add(Cookie cookie)
    public int  LiveCount(long nowUnixSec)
    public void IngestFromResponse(HttpResponse response, string responseHost,
                                   string responsePath, long nowUnixSec)
    public void IngestSetCookieHeaders(HttpHeaders headers, string responseHost,
                                       string responsePath, long nowUnixSec)
    public string CookieHeaderFor(string host, string path, bool isSecure, long nowUnixSec)
    public void SaveToFile(string path)
    public int  LoadFromFile(string path)       // 返回载入条数
}
```

时间一律由调用方以 Unix 秒传入——jar 自己不读时钟。挂到 `HttpClient.SetCookieJar` 之后
吃 / 带 cookie 是自动的，手工调 `Ingest*` / `CookieHeaderFor` 只在脱离 `HttpClient`
用时才需要。

---

## Std.Net.WebSockets

### 常量

```z42
WebSocketState.Connecting = 0;  Open = 1;  CloseSent = 2;  Closed = 3
WebSocketMessageType.Text = 1;  Binary = 2;  Close = 8;  Ping = 9;  Pong = 10
```

### WebSocketClient

```z42
public WebSocketClient()
public int  State()
public void Connect(string url)                      // 仅 ws://；wss:// 抛 NotSupportedException
public void SendText(string text)
public void SendBinary(byte[] data, int length)
public void SendPing(byte[] payload, int length)
public WebSocketMessage Receive()                    // 阻塞
public void Close(int statusCode, string reason)
public void Dispose()
```

`Receive()` 内部自动处理控制帧：收到 Ping 自动回 Pong 并继续等，Pong 直接丢弃，
收到 Close 帧则把状态切到 `Closed` 并返回一个 `Close` 类型的消息。
`Close(code, reason)` 发完关闭帧就直接断开 TCP，**不等对端的关闭确认**。

### WebSocketMessage

```z42
public int    MessageType;
public byte[] Buffer;

public WebSocketMessage(int messageType, byte[] buffer)
public bool   IsText()
public bool   IsBinary()
public bool   IsClose()
public string AsString()        // UTF-8 解码 Buffer
public int    CloseStatus()     // 仅 Close 消息有意义
public string CloseReason()
```

### WebSocketServer / WebSocketConnection

```z42
public class WebSocketServer {
    public WebSocketServer()
    public void Bind(string host, int port)
    public WebSocketConnection Accept()      // 阻塞，内含 RFC 6455 握手
    public int  LocalPort()
    public void Stop()
}

public class WebSocketConnection {
    public WebSocketConnection(TcpClient tcp, Stream stream)
    public int  State()
    public void SetPermessageDeflate(bool enable)
    public bool GetPermessageDeflate()
    public void SendText(string text)
    public void SendBinary(byte[] data, int length)
    public void SendPing(byte[] payload, int length)
    public WebSocketMessage Receive()
    public void Close(int statusCode, string reason)
    public void Dispose()
}
```

⚠️ **服务端目前收不到客户端消息**：`WebSocketConnection.Receive()` 对任何带掩码的入站帧
抛 `WebSocketProtocolException: server frame is masked (RFC 6455 §5.1 violation)`，
而 RFC 6455 §5.3 要求客户端→服务端的帧**必须**带掩码。握手、`LocalPort`、服务端发送方向
都正常，只有接收方向不可用。

---

## 用法

### TCP 回环

```z42
using Std.IO;
using Std.Net.Sockets;
using Std.Threading;
using Std.Encoding;

void Main() {
    var lis = new TcpListener();
    lis.SetReuseAddress(true);          // 必须在 Bind 之前
    lis.Bind("127.0.0.1", 0);
    int port = lis.LocalPort();

    var t = Thread.Start(() => {
        var peer = lis.AcceptTcpClient();
        var ps = peer.GetStream();
        byte[] buf = new byte[64];
        int n = ps.Read(buf, 0, 64);
        ps.Write(buf, 0, n);            // echo
        peer.Dispose();
    });

    var c = TcpClient.ConnectTo("127.0.0.1", port);
    c.SetNoDelay(true);
    c.SetReadTimeout(3000);
    var s = c.GetStream();
    byte[] msg = Utf8.GetBytes("ping");
    s.Write(msg, 0, msg.Length);
    byte[] rb = new byte[64];
    int got = s.Read(rb, 0, 64);
    Console.WriteLine($"echoed {got} bytes");
    c.Dispose();
    t.Join();
    lis.Stop();
}
```

### UDP 回环

```z42
var rx = new UdpClient();
rx.Bind("127.0.0.1", 0);
rx.SetReadTimeout(3000);
int port = rx.LocalPort();

var tx = new UdpClient();
byte[] d = Utf8.GetBytes("hello-udp");
tx.Send(d, d.Length, "127.0.0.1", port);     // 自动 bind

var res = rx.Receive();
Console.WriteLine($"{Utf8.GetString(res.Buffer)} from {res.RemoteHost}:{res.RemotePort}");
rx.Dispose();
tx.Dispose();
```

### HTTP 服务端 + 客户端

```z42
var srv = new HttpServer();
srv.Bind("127.0.0.1", 0);
int port = srv.LocalPort();

var t = Thread.Start(() => {
    srv.Serve((ctx) => {
        if (ctx.Request.Url == "/json") { ctx.SendJson(200, "{\"ok\":true}"); }
        else { ctx.SendText(200, "method=" + ctx.Request.Method); }
    });
});

var cli = new HttpClient();
cli.SetTimeout(5000);
var r = cli.Get($"http://127.0.0.1:{port}/json");
Console.WriteLine($"{r.StatusCode} {r.BodyAsString()}");   // 200 {"ok":true}
cli.Dispose();
srv.Stop();
```

---

## 不支持

- **async / await、非阻塞 socket、selector / epoll**：一切都是阻塞调用 + 线程
- **HTTP/2、HTTP/3**：只有 HTTP/1.1
- **服务端 TLS**：`HttpServer` 只跑明文 http；没有 `TlsListener`
- **`wss://`**：WebSocket 客户端只接 `ws://`
- **WebSocket 服务端收消息**：见上文掩码帧的警告
- **HTTPS 的流式响应体**：`SendStreaming` 遇 `https://` 抛 `NotSupportedException`
- **HTTPS 连接池**：keep-alive 池只覆盖明文连接
- **客户端证书 / 自定义证书校验 / 系统信任库**：只信内置 Mozilla 根集
- **URL 里的 IPv6 字面量**：`http://[::1]/` 解析失败
- **URL 里的 userinfo**：`http://u:p@h/` 抛 `NotSupportedException`
- **IPv6 zone id**：`fe80::1%en0` 解析失败
- **`IPEndPoint` 作为 socket 参数**：所有 socket API 收的是 `(host, port)`
- **DNS 的记录类型查询与超时控制**：只有 `GetHostAddresses`
- **HTTP 服务端的路由、keep-alive、流式请求体、WebSocket upgrade**
- **代理（HTTP_PROXY / SOCKS）**
- **wasm32 上的任何 socket**：抛 `NetUnsupportedException`

`NetTcpDecode` / `UdpDecode` / `_FrameCodec` / `_HttpRequestParser` / `_HttpBodyStream` /
`_LineReader` / `_WsLineReader` / `_DecodedFrame` 以及 `NetworkStream` / `TlsStream` 的
`MarkClosed`、`TcpClient(long, bool)` 构造器虽然是 `public`，都属内部实现载体，
不构成稳定 API。
