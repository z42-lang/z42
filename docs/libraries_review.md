# z42 标准库审查报告（2026-07-05）

> 覆盖 `src/libraries/` 下全部 24 个包（22 入编 + z42.build / z42.project 两个 parked），
> 从**模块划分 / API 实现 / 机制扩展 / 性能特性**四个角度审查，另附**横切一致性**核对。
> 每条发现附证据位置（`包/src/文件:行`，均相对 `src/libraries/`）与建议做法。
> 本报告为改进 backlog 输入，落实时按 workflow.md 各自拆 fix / refactor / docs change。

---

## 0. 执行摘要（按影响优先级 Top 建议）

按「影响 / 成本比」排序的动手顺序：

| # | 项 | 类型 | 位置 | 说明 |
|---|----|------|------|------|
| 1 | Mutex/RwLock 回调抛异常 → 锁永不释放（死锁） | fix | `z42.threading/src/Mutex.z42:33-38`、`RwLock.z42:37-83` | 唯一可致生产死锁项；加 try/finally，~10 行 |
| 2 | Queue/Stack 空容器 Dequeue/Pop 静默损坏状态 | fix | `z42.collections/src/Queue.z42:36-46`、`Stack.z42:33-40` | 返回陈旧值 + count 变 -1；同包 PriorityQueue/LinkedList 已抛异常 |
| 3 | AES 每轮重建 256 项 S-box、SHA/BLAKE/Keccak 每块重建常量表 | perf | `z42.crypto/src/Aes.z42:897-906`、`Sha256.z42:106`、`Sha3.z42:169-170` 等 | 提为 static 一次性表；crypto 全线最高杠杆 |
| 4 | `CharAt` 每次 O(n) UTF-8 解码 → 全库字符串操作实际 O(n²) | perf/机制 | `z42.core/src/Strings.z42` 头注释自认 | 需 O(1) char-index perf spec + `ToCharArray()`；全 stdlib 最高杠杆 |
| 5 | extern 规则与现实全面脱节（9 包在 core/io 外声明 native extern） | 裁决 | 见 §5.1 | 规范冲突，须 User 裁决规则走向后统一审计表 |
| 6 | json/toml/yaml 三份 Value 树 + escape + 错误模型复制且正确性漂移 | refactor | 见 §1.1 | 抽共享值模型 + TextCursor + escape 基建 |
| 7 | ECDSA/Ed25519 秘密标量乘变时序、泄漏 nonce 位长 | fix(安全) | `z42.crypto/src/EcdsaP256.z42:323-345`、`Ed25519.z42:248-264` | 已知格攻击向量；固定迭代次数 ladder |
| 8 | 三格式 parser 无深度上限 → 恶意嵌套打爆 VM 栈 | fix(安全) | JSON/TOML/YAML parser + writer 共 6 处 | 统一加 `_depth` 上限 |
| 9 | 12+ 个源文件超 500 行硬限（BigInt 2198 / YamlParser 1647 / HttpClient 1406） | refactor | 见 §6 | 逐个独立 refactor change |
| 10 | 顶层 + 各包 README 大面积失真 | docs | 见 §7.1 | crypto/net README 与代码直接矛盾 |

---

## 1. 模块划分

### 1.1 三格式（json/toml/yaml）Value 树 + 解析基建三重复制（最大结构性债务）
`JsonValue`(284) / `TomlValue`(288) / `YamlValue`(406) 是「int kind + typed 槽位 + 并行数组 + 手写扩容」同构体，`Set` 方法几乎字符级相同：`z42.json/src/JsonValue.z42:200-229` vs `z42.toml/src/TomlValue.z42:202-232` vs `z42.yaml/src/YamlValue.z42:315-344`。解析器 cursor 基建（`Peek/Advance/Eof/Err/TryLiteral/HexDigit`）五处复制：JsonParser / TomlParser / YamlParser / RegexParser / Uri 各一份。escape `\uXXXX` 解析三份且**三种正确性水平**（见 §2.5）。
- **建议**：抽共享树模型（如 `z42.data` 的 `Value`），三格式只保留 kind 扩展 + 包装入口；z42.text 落一个带行列的 `TextCursor`（+TryLiteral+hex）供五包复用；escape 编解码共享 helper「一次修对处处对」。JsonPath 等工具随之免费适用三格式。

### 1.2 `List<T>` 约束过强，numerics 类型全被挡在集合门外
`z42.core/src/Collections/List.z42:12` 要求 `where T: IEquatable<T> + IComparable<T>`，而 `BigInt`(`z42.numerics/src/BigInt.z42:35`)、`Decimal`、`Complex` 实现了 `CompareTo`/`Equals` 却**未声明**接口 → `List<BigInt>`、`SortedSet<Decimal>` 无法实例化。
- **建议**：numerics 三类型补声明接口；`List<T>` 约束下放到方法级（只有 `Sort`/`Contains` 需要），否则任何不可比较类型都进不了 List。

### 1.3 numerics→random 依赖边可消除（解开 crypto 死结）
`z42.numerics.z42.toml` 依赖 z42.random 仅为 `IsProbablyPrime`/`NextPrime` 内部 `new Random()`（`BigInt.z42:1109,1195`），该边已阻塞 crypto（crypto→numerics→random→crypto 成环，`z42.random.z42.toml` NOTE 已记录）。
- **建议**：`NextPrime` 改用同文件已有的确定性 `IsPrime`/`IsBpswPrime`，删默认 RNG 版或强制传 `Random`，摘掉此边。

### 1.4 z42.threading → z42.diagnostics 分层倒置
底层并发原语依赖日志门面（仅为 Timer 回调吞异常时 `Log.Error`），使 threading 传递依赖 io+time，且导致 **diagnostics 永远无法用 threading**（成环）。BCL/Rust 均不让 Thread/Mutex 依赖 logging。
- **建议**：Timer 错误改为回调式 error handler 或静默，摘掉此边。

### 1.5 z42.net 内联 HttpUrl 重复 z42.uri 的 URL 解析
`z42.net/src/Http/HttpUrl.z42:12-13` 注释自认「用内联 parser 避免 cross-package 依赖噪声」，RFC 3986 子集解析在 `z42.uri/src/Uri.z42`(770) 与 HttpUrl 两处并存；且 HttpUrl 不支持 IPv6 字面量主机，而同包 `IPAddress` 已完整支持 IPv6（能力割裂，`HttpUrl.z42:16,78`）。
- **建议**：HttpUrl 改依赖 z42.uri。

### 1.6 归档 vs 压缩：Tar/Zip 应拆出 z42.archive
`Tar.z42`/`Zip.z42` namespace 是 `Std.Archive` 却住在 z42.compression 包（其余文件 `Std.Compression`）。归档≠压缩（对标 Rust tar/zip 独立 crate）。
- **建议**：拆 `z42.archive` 包，顺带解决 Tar.z42 超行数问题。

### 1.7 net 包 socket 层平铺过散
`Http/`、`WebSockets/` 已分目录，但 TCP/UDP/TLS/DNS/IPAddress/IPEndPoint 14 文件平铺在 `src/` 根。
- **建议**：`src/Sockets/`(Tcp/Udp/Network/IP*)、`src/Tls/`、`src/Dns/` 分目录。

### 1.8 其它模块划分小项
- `Std.Collections` 横跨 core + z42.collections 但契约不闭合：core 用 `Count` **字段**，z42.collections 用 `Count()` **方法** + `IBasicCollection`，主力 List/Dictionary 恰好无法实现契约（§8.4 一致性也提及）。
- `_MagDivModResult`（`BigInt.z42:26-33`）以 public 污染 `Std.Numerics`；`_GrowBuf`（`Zip.z42:424-488`）以 public 污染 `Std.Archive` 且与 MemoryStream 重复 → 降为文件私有。
- `Math.Sinh/Cosh` 缺失迫使 `Complex.z42:213-227` 跨包手搓（注释自认）。
- 单文件多 public 类型：`z42.uri/src/Uri.z42` 一文件装 Uri+UriParser+UriCodec 三类。
- `UriCodec` 自带弱化版 UTF-8 编解码（`Uri.z42:700-768`），与 z42.encoding.Utf8 职责重叠 → 改调 z42.encoding。
- `Encoding._kind` 是死字段，`new Encoding(2)` 也走 UTF-8（`Encoding.z42:21-48`），Utf16/Utf32 编解码器同包存在却接不进 → StreamReader/Writer 的 encoding 参数徒有其表。
- `__time_now_ms` 双重绑定：`Environment.GetCurrentTimeMs` 与 `DateTime.NowMs` 绑同一 native，墙钟职责应归 z42.time。

---

## 2. API 实现（正确性隐患 + 缺口 + 不一致）

### 2.1 状态损坏 / 崩溃类（最高优先）
- **Queue/Stack 无空检查**：`Queue.z42:36-46`（Dequeue 读 `items[head]`、count 减到 -1、返回陈旧值）、`Stack.z42:33-40`。同包 PriorityQueue/LinkedList 都抛异常 → 同包不一致 + 数据损坏。
- **Dictionary.FindSlot 对 `GetHashCode()==int.MinValue` 崩溃**：`Dictionary.z42:128-130` `if (h<0) h=-h;`，`-int.MinValue` 溢出仍为负 → 负索引 trap。改 `h & 0x7FFFFFFF`。
- **BigInt.Equals(object) 无类型检查强转**：`BigInt.z42:1639-1644` `BigInt b=(BigInt)other;`，`bigint.Equals("x")` cast 失败炸而非返回 false。
- **Assert.Equal(null, x) 直接 NPE**：`z42.core/src/Assert.z42:9-13` 未判 null；`z42.test/src/Assert.z42:34` 同款，抛的不是 TestFailure。
- **String.Substring 负参/越界无干净报错**：`String.z42:153-160`，`Substring(2,-1)` 走 `new char[-1]` trap。
- **MemoryStream.Seek 静默截断**：`MemoryStream.z42:123` `(int)newPos` 超 i32 回绕无检查。

### 2.2 数值 / 时间正确性
- **JsonParser 大整数溢出注释与代码不符**：`z42.json/src/JsonParser.z42:368-370` 注释称「catch 回退 f64」但**无 catch**，超 i64 合法 JSON 直接抛裸异常（非 JsonException、无行列）。补 catch→`OfDouble` + 回归测试。
- **Double.CompareTo(NaN)==0 破坏全序**：含 NaN 的 `List<double>.Sort()` 未定义；C# 语义 NaN 排最前。
- **ParseIso8601 不校验日对月上限**：`DateTime.z42:313-315` 只查 1..31，`"2026-02-31"` 静默滚成 3/3；同文件 `DateTime.Utc` 却严格拒绝（`:130-134`）。
- **TOML 数字扫描过度宽松**：`TomlParser.ParseNumber:734-755` 放行 `1.2.3`/`1e5e5`/前导零 `042`，失败抛的不是带行列的 TomlException。
- **chunk-size 用带符号 int 累加可溢出**：`HttpClient.z42:1261-1281` 等三处，正向 wrap 放行错误尺寸。用 long + 显式拒绝 `>2^31-1`。

### 2.3 安全正确性（crypto / net）
- **ECDSA 签名变时序 + 泄漏 nonce 位长**：`EcdsaP256.z42:323-345` 以 `k.BitLength()` 为循环上界 + `TestBit` 条件加（已知格攻击向量）；secp256k1 同构。改固定 256 迭代 Montgomery ladder。
- **Ed25519 签名标量乘变时序，且与 X25519 策略矛盾**：`Ed25519.z42:248-264` 变时序无声明，而同包 `X25519.z42:97-149` 是恒定时间 ladder + `_cswap`。
- **AES-CBC PKCS#7 拆填非常数时间 → padding oracle**：`Aes.z42:204-217` 非法 pad 抛不同异常消息、循环长度依赖 padLen。统一错误 + 恒定时间遍历。
- **HTTP `Transfer-Encoding` 精确等值判断**：`HttpClient.z42:1098` 等 `te.ToLower()=="chunked"`，漏 `gzip, chunked` 组合 → body 解析错误。取末项 trim 判断。
- **无重复 Content-Length / TE+CL 冲突校验 → 请求走私**：`HttpHeaders.z42:53-63` 只返首值，不拒 chunked+CL 共存（RFC 7230 §3.3.3）。
- **AEAD 无 nonce 唯一性防护 / 无随机 nonce 入口**：CTR/GCM/ChaChaPoly nonce 复用灾难性，已有 SecureRandom 未接入（`Aes.z42:89-124`、`ChaCha20Poly1305.z42:28-51`）。
- **缺公开常数时间比较 API**：AEAD 内部各手写 ct-compare 5 处（`ChaCha20Poly1305.z42:80-85`、`Aes.z42:301-306` 等），未对外暴露 → 用户校验 HMAC/token 只能用 `==` 时序泄漏。加 `public static bool ConstantTimeEquals`。
- **WebSocket 入站无 UTF-8 校验 / 无分片重组 / 无 payload 上限**：`WebSocketConnection.z42:110-158`（RFC 6455 §8.1）。
- **HTTP 重定向 https(443) 错误加 `:443`**：`HttpClient.z42:331` 用 `Port != 80` 判断。

### 2.4 资源安全 / 数据错位
- **Mutex/RwLock 回调抛异常 → 死锁**：`Mutex.z42:33-38`、`RwLock.z42:37-83` 无 try/finally，runtime guard `mem::forget` 靠显式 unlock（`src/runtime/src/corelib/sync.rs:136-142`）。类头注释与实现相反。加 try/finally。
- **BufferedStream 读→写切换写错位置**：`BufferedStream.z42:137-146` 只丢 read buffer 不 Seek 回退 inner，后续 Write 落错位置。CanSeek 时回退。
- **BinaryReader 异常包装失效**：`BinaryReader.z42:119-127` catch `InvalidOperationException`，但 `Stream.ReadExactly` 已改抛 `EndOfStreamException`（`Stream.z42:155`）→ ReadByte 抛 BinaryException 而 ReadBytes 泄漏 EndOfStreamException，同类两种 EOF 异常。

### 2.5 编码往返破坏（archive / escape）
- **Zip 文件名编码往返破坏**：写侧 `_utf8Bytes` 自制编码器只 3 字节分支却声称支持 supplementary（`Zip.z42:273-308`），非 BMP 编错；读侧 `_ReadStr` 按 Latin-1 `(char)bytes[i]`（`:403-416`），与写侧设的 UTF-8 标志位矛盾。改依赖 `Std.Encoding.Utf8`。
- **Tar 文件名伪 Latin-1**：`Tar.z42:402-421,470-480` 码点 >255 静默截断。
- **YAML `\uXXXX` 不处理 surrogate pair**：`YamlParser.z42:717-736` 直接 `(char)code`；JSON 侧已正确（`JsonParser.z42:256-271`），TOML 侧连 surrogate 区间都不拒（`TomlParser.z42:657`）→ 同一能力三格式三种行为。
- **writer 输出非法/不可回读文本**：`TomlWriter.z42:158-174` QuoteBasic 不逃逸 `\0 \b \f` 等 <0x20；`YamlWriter._DoubleQuote:228-244` 同病（JSON `\u00XX` 兜底是正确范本）。
- **YAML round-trip 类型漂移**：`"2024-01-01"`/`"0xFF"`/`"0o755"` 被 `_NeedsQuoting` 判免引号裸输出，再 Parse 变 timestamp/int（`YamlWriter.z42:165-201` vs parser `_LooksLikeTimestamp:1347`）。
- **UriCodec.Decode 无条件 `'+'→空格`**：`Uri.z42:668-672` 是 form-urlencoded 专属语义，对通用 DecodeComponent 错误。拆 `DecodeComponent`/`DecodeFormComponent`。

### 2.6 API 缺口（对标 BCL/Rust，低成本可补）
- `Char.IsDigit/IsLetter` 缺失：`BigInt.Parse`(`:109`)、`Decimal.Parse`(`:86`) 都手写 `ch<48||ch>57`。
- `string.ToCharArray()` 缺失（有 FromChars 无逆）→ 放大 §4.1 的 O(n²)。
- `TryParse` 全线缺失（Int32/64/Double.Parse、Convert.To* 只有抛异常版）。
- `Int32.MinValue/MaxValue` 等常量缺失 → BigInt/Random 到处硬编码字面量。
- `Math` 缺 `Asin/Acos/Atan/Sinh/Cosh/Tanh/Truncate/Sign/Clamp`（Clamp/Sign 是三行脚本）。
- `HashSet<T>` 缺失（有 SortedSet），去重只能 `Dictionary<K,bool>` 凑。
- `Queue/Stack/LinkedList` 无 `ToArray()`/无法遍历（不破坏结构无法枚举）。
- `List<T>` 缺 `AddRange/ToArray/Sort(IComparer)`；`IComparer` 接口声明用途从未兑现。
- `Dictionary` 缺 `TryGetValue`，`Get` 未命中返 default 而非抛 `KeyNotFoundException`（该异常类躺在 `Exceptions/` 无人用）。
- `Stream` 缺 `ReadByte()/WriteByte(b)`（BinaryReader 每次 `new byte[1]` 是代偿）。
- `TimeSpan` 缺 `FromDays` / 分量访问器 / `Negate`，`ToString` 只输出 `"...ns"`。
- `File` 缺 `ReadAllLines/ReadLines`；`Path` 缺 `GetFullPath/Normalize`。
- hash 只有一次性 API，无流式 `Update/Final`（大文件必须整体载入，见 §3）。
- CLI 缺 `--` 分隔符（`ArgParser.z42:306-331`），负数 positional `-5` 抛 unknown option。
- Regex 无静态便捷 `Regex.IsMatch(pattern, input)`；三格式无 TryParse/Result 型 API。

### 2.7 错误处理不统一（跨包）
- **错误处理三套并存**：core String/collections 抛裸 `Exception`（`String.z42:105,232`、`LinkedList/SortedSet` 6 处），numerics/random 用类型化异常，`InvalidOperationException` 全库无人用。立规则：空容器→InvalidOperationException、坏参数→ArgumentException，回改 core。
- **z42.crypto 无 CryptoException**：6830 行全用 ArgumentException(84)+InvalidOperationException(4) + 一处裸 `throw new Exception("unreachable")`（`SecureRandom.z42:79`）。
- **同包同语义不同异常**：`Decimal.DivideBy` 抛 ArgumentException（`:192-194`）vs `BigInt._divMod` 抛 DivideByZeroException（`:413-415`）；z42.time `ParseIso8601` 抛 ArgumentException vs `DateTimeOffset.Parse` 抛 FormatException。
- **格式包错误报告结构不统一**：Json/Toml 异常带 `Line/Column` 字段，YamlException 无字段（位置拼进 message），Uri/Regex/Cli 异常无位置。
- **z42.test Runner 静默吞 Setup/Teardown 异常**：`Runner.z42:99` `catch(Exception ex){}` → Setup 崩溃后测试照跑、可能假 PASS。
- **长度 API 三种拼法**：`String.Length`(属性)/`List.Count`(字段)/`Queue.Count()`(方法)/`StringBuilder.GetLength()`(方法)。
- **数值 API 命名不统一**：Json/Toml 用 `OfLong/AsLong`+`OfDouble/AsDouble`，Yaml 用 `OfInt/AsInt`+`OfFloat/AsFloat`。

---

## 3. 机制扩展（缺失横切机制 → 重复 / hack）

### 3.1 foreach 只走鸭子协议，IEnumerable 半吊子（解锁集合层的钥匙）
`Protocols/IEnumerable.z42` 注释承认 foreach 走 `Count + get_Item` 索引协议、接口「主要用于泛型约束」。后果：Dictionary 只能以 `Keys()/Values()/Entries()` 三份全量拷贝快照模拟遍历（`Dictionary.z42:83-124`），LinkedList/Queue/Stack 完全不可 foreach，Directory 全量物化返回 `string[]`。
- **建议**：foreach 升级到 IEnumerator 路径，优先级提前——是解锁整个集合层的钥匙。

### 3.2 IDisposable 断层 → using 无法落地
core 有 `IDisposable`+`Disposable` 工厂，`ProcessHandle` 已实现，但 Stream 家族全部只有裸 `Close()`，调用侧充满手写 try/finally+Close（`Tar.z42:240-246`）。`TextReader.Dispose` 注释仍写「mirrors the *future* IDisposable shape」（已过时）。
- **建议**：Stream/Reader/Writer 统一实现 IDisposable（Dispose→Close），是 using 语句前置。

### 3.3 缺 HashAlgorithm / IAead 抽象 → crypto ~2000 行样板重复
- HmacSha256/Sha1/Sha512/Sha384/Md5 五类逐字节复制 ipad/opad 结构（`Hmac.z42:24-435`，5×~85 行）；HKDF 三类 Extract/Expand 同构（`Hkdf.z42:27-230`）；所有 hash 的 `HashString/HashHex` 包装逐个手抄。
- 两 ECDSA 类近乎完整复制：`EcdsaP256.z42`(474) 与 `EcdsaSecp256k1.z42`(446) 仅 curve 常量 + 倍点不同（`EcdsaP256.z42:26` 注释自认）。
- GCM/CCM/ChaCha20Poly1305 三套 AEAD 无共同 `IAead`，tag 比较/`ct||tag` 拼装/pad16 各写一遍。
- LE/BE 字节⇄整数 I/O 在十余文件重复（`_readLE64/_lshr64/_rotr64` 等，`_lshr64` 至少 6 处逐字复制）。
- **建议**：定义 `HashAlgorithm{BlockSize/DigestSize/Hash}` → HMAC/HKDF/PBKDF2 一份通用实现；`WeierstrassCurve`+`EcdsaCore` 参数化两曲线；`IAead`+共享 `_pad16/ConstantTimeEquals`；crypto 内建 `_Bits`/`_ByteOps` util。

### 3.4 缺泛型 delegate / 泛型静态方法 → 复制粘贴
- Multicast 三胞胎 ~1000 行复制：`MulticastAction.z42`(303)/`MulticastFunc.z42`(302)/`MulticastPredicate.z42`(376) 双 vec 通道 + Subscribe/Grow/COW 逐行相同。
- `Random.Shuffle/Sample` 按元素类型三份复制（ShuffleInt/Long/String、SampleInt/String），算法完全一致。
- **建议**：泛型 delegate 抽象 + 返回值策略；泛型方法就绪后合并为 `Shuffle<T>(T[])`。

### 3.5 「并行数组 + 手写 grow-copy」样板在 7 包出现 20+ 次
根因是「类字段不支持泛型参数」（`TomlValue.z42:14-17` 自述）。JsonValue/TomlValue/YamlValue、YamlParser anchors、RegexParser、ArgParser 三组 EnsureCap、SubcommandRouter._grow 等。
- **建议**：z42.collections 先落非泛型 `ObjectList/StringList` 顶上，编译器修好字段泛型后统一替换。

### 3.6 Exception 子类 ToString 样板 ×13
每子类手写 `override string ToString(){ return "XxxException: "+this.Message; }`（`ArgumentException.z42:7-9` 等 13 处）。基类有 `GetType()` → `Exception.ToString` 改 `this.GetType().Name+": "+this.Message` 即可全删。

### 3.7 编译器 overload-resolution 缺陷外溢成一片 API 疤痕
`OverStream` 静态工厂替代 `(Stream)` 构造器（`BinaryReader.z42:34-49`）、`WriteStdinString` 而非重载（`ProcessHandle.z42:74-83`）、StringWriter 禁 `Write(char)`、三份几乎相同的行读取器因「跨文件 class-token 问题」内联复制（`WebSocketClient.z42:481`、`HttpClient.z42:1315`、`_HttpRequestParser.z42:229`）、z42.test `Array` 前缀命名。
- **建议**：这些都指向编译器债 `compiler-future-typed-overload-resolution`，修一次消一片 → 作为根因项推动编译器修复，而非长期多份复制。

### 3.8 其它机制项
- `INumber<T>` 已存在但 BigInt/Decimal/Complex 不实现（停留在 `a.Add(b)` 命名方法，`BigInt.z42` 注释归因 operator overload 延后）→ 实现后泛型数值算法覆盖 int/double/BigInt/Decimal。
- `IFormattable` 声明后零实现，`String.Format` 注释称「待 IFormattable 引入」但协议早已在，是消费端从未接线。
- 三份 Base32 拷贝（`Base32/Base32Hex/Base32Crockford` 40-bit 主循环逐行同构）→ 参数化 5-bit 核心砍 ~250 行。
- TextReader/TextWriter 是零实现者的死抽象（grep 全包无 `: TextReader`），基类多态面落空 → 让四子类挂上或删这 214 行。
- regex 无 step budget / timeout（`Regex.z42:8-10` 只文档警告灾难回溯）→ 加步数计数器抛 RegexException。
- z42.test 断言/运行器局限：`TestFailure.Location` 永远空（`Failure.z42:29-31`）、`ArrayContains` 用 `==` 引用比较（`Assert.z42:325-335`）、无按名过滤 / per-test 计时、v0 imperative 与 v2 reflective 双运行器并存同包。
- CLI option 修饰不可组合（AddOption/WithEnv/Required/Repeated 四入口各填并行数组，required+env 无法表达；`ParseResult` 字段全 public 封装泄漏）。
- 无编译/解析缓存：`JsonPath.Select` 每次重解析 path，Regex 无静态缓存入口。

---

## 4. 性能特性

### 4.1 字符串 O(n²)（全 stdlib 最高杠杆）
- **`CharAt` 每次 O(n) UTF-8 解码**：`Strings.z42` 头注释自认 helper 全走 `s.CharAt(i)` → 「single walk」实为 O(n²)。受害面：String.IndexOf/Replace/Split/Trim/ToLower、`StringBuilder.ToString`（`:66-78` 逐字符）、`Levenshtein.Distance`（`:44` 每格解码）。**建议**：O(1) char-index perf spec；配合 `ToCharArray()` 让脚本侧先自救（Levenshtein 预转 char[] 即降一维）。
- **循环内 `acc = acc + …` O(n²) 拼接**：`String.Join`(`:304-312`)、`Regex.Replace`(`:167-178`)/`_ExpandReplacement`、`SubcommandRouter.HelpText`(`:185-197`)/`_padRight`、`LogFields.Format/_escape`(`:46-80`)、`Ansi.Strip`(`:111-116`)。改 StringBuilder（同文件 Replace 已示范两遍扫描 + 一次分配）。
- **逐字符 `char[1]+FromChars` 分配热路径**：`JsonParser.ParseString:243-246`、`TomlParser:508-511`、两 writer；YAML 已证明 `sb.Append(c)` char 重载可用 → Json/Toml 的 `CharToString` 纯多余分配。无转义段应整段 Substring。

### 4.2 查找表每次调用重建（crypto 最高杠杆）
- **AES 每轮重建 256 项 S-box**：`Aes.z42:897-906` `_subBytes`/`_invSubBytes` 内 `Aes._sbox()`，`_keyExpansion` 循环内(`:843,849`)也调；单块加密重建 10+ 次。
- **SHA/BLAKE/Keccak 每块重建常量表**：`Sha256.z42:106`(`new long[64]`)、`Sha512.z42:134`(`long[80]`)、`Sha3.z42:169-170`(每 24 轮建两表)、`Blake2b.z42:172`(`_sigma` `int[160]`)。
- **Zip CRC-32 每次调用重建 256 项表**：`Zip.z42:259-261` `_crc32` 内 `Zip._crc32Table()`。
- **建议**：全部提为 static 一次性表（或惰性单例）。

### 4.3 BigInt 大数算法
- **Poly1305/RSA/ECDSA 逐字节 ShiftLeft 累加**：`Poly1305.z42:97-110` 每字节 `new BigInt(b).ShiftLeft(8*i)` 从零构造 → O(n²)；Rsa/Ed25519/X25519 同构。改 Horner `acc.ShiftLeft(8).Add(byte)`。
- **BigInt.ToString/ToHex/ToBase 输出 O(位数²) 拼接**：`BigInt.z42:261-272,319-326` 逐字符 `s=s+...`，chunk 已在 int[] → 一次性 char[] 填充。
- **BigInt.Parse 每位一次全长乘法**：`BigInt.z42:107-116` per-digit `Multiply(ten)+Add`，O(位数²)；ToString 已用 10⁹ chunk，Parse 对称按 9 位一组少 ~81× limb 操作。
- **Decimal._powTen 线性乘法**：`Decimal.z42:147-156` 乘 n 次 10 → `ten.Pow(n)`（隔壁 BigInt.Pow 平方求幂）。

### 4.4 值树 / 集合 O(n²)
- **JSON/TOML/YAML 值树 O(n²) 构建与序列化**：对象 `ContainsKey/Get/Set` 全线性扫（`JsonValue.z42:178-229` 解析 n 键每次 Set 先查重 → O(n²)；`JsonWriter.WriteObject:76-78` 每键再 `Get` 线性查）。千键文档（k8s manifest / lock 文件）显著劣化。大表切排序索引/哈希，或 writer 遍历并行数组避免二次查找。
- **Dictionary.Remove 探测链重插走完整 Set**：`Dictionary.z42:59-67` 链上每项经 Set→负载检查→FindSlot；标准做法仅槽位回填。`Clear`/`RemoveAt`/`Pop` 不清引用槽（`List.z42:47-54,74`、`Stack.z42:33-36`）拉长 GC 存活期。
- **List.Sort 插入排序 O(n²)**：`List.z42:76-89`（注释自认），有 IComparer 后升级归并/快排。
- **Path._sortStrings 插入排序 O(n²)**：`Path.z42:232-244`，`GlobRecursive` 全树扫描退化。

### 4.5 IO / 网络无缓冲 / 反复分配
- **HTTP chunked/EOF body 反复整体拷贝扩容**：`HttpClient.z42:1151-1158,1200-1210` 每 chunk `new byte[cap]` 全量复制 → O(n²)；请求侧 `_HttpRequestParser.z42:137-144` 同。用分段 buffer 列表最后拼接。
- **StreamReader 全流物化**：`StreamReader.z42:117-123` 首读即 `ReadAllBytes()` 整流进内存，tail 大文件不可用。chunked decoder 应提优先级。
- **BinaryReader/Writer 每字段一次堆分配**：`ReadByte` `new byte[1]`(`:104`)、每 `ReadInt*`/`WriteInt*` 新数组。实例级 scratch buffer 消除。
- **regex 回溯二次代价**：`Regex.z42:486-503` `MatchQuant` 每回溯候选从头重跑 child；`FindFrom:266-274` 每起始位置 new 两数组 → 长文本 2n 分配。
- **DateTime.AddMonths 三次日历分解**：`DateTime.z42:188-190` `Year()/Month()/Day()` 各重跑 `_civilFromDays` → 一次分解取三值。
- `MulticastX.Count()` 每次全数组扫描（`MulticastAction.z42:74-`）→ 增量维护 int。
- `Path.Slice` 逐字符复制重实现 Substring（`Path.z42:251-258`）；`Base64Url` 编解码各做 2-3 次全串 Replace。

---

## 5. extern 规则冲突（需 User 裁决 —— 规范冲突检测条款）

### 5.1 现状：9 个包在 core/io 之外声明 native extern
`src/libraries/README.md:52-66` 规则「VM extern 只准 z42.core，native FFI 只准 z42.io」，实测违反：

| 包 | 位置 | 内容 |
|----|------|------|
| z42.net | `NetTcpNative.z42`(16)、`UdpNative.z42`(12)、`TlsClient.z42`(6)、`Dns.z42:47`、`HttpClient.z42:958`(`_timeNowMs`) | socket/TLS/DNS native |
| z42.threading | Thread/Mutex(4)/RwLock(8)/Channel(7) | 线程/锁/通道 native |
| z42.compression | Zstd/Lz4/Brotli/Deflate/Zlib/Gzip/Encoder-Decoder-Stream 8 文件 | cdylib native |
| z42.crypto | `SecureRandom.z42:18-19`(`_CryptoRandomBytes`) | OS CSPRNG |
| z42.test | `TestIO.z42:71-80`、`Bencher.z42:135,152`、`ModuleLoader.z42:67,75` | VM builtin |
| z42.time | `Stopwatch.z42:12`、`DateTime.z42:14` | VM builtin |
| z42.math | `Math.z42:54-65`(libm 12) | libm |
| z42.io.binary | `BinaryReader.z42:205-208`、`BinaryWriter.z42:192-195`(float bit-cast) | VM builtin |

规则文本停留在只有 core+io 的时代；同一 README「第 2 节规则」与「extern 审计表」（承认 math libm）也互相矛盾；`overview.md:80` 又称未来 compression/crypto 允许 extern，与硬规则冲突；net/crypto/tls 从未列入任何白名单或审计表。
- **按 CLAUDE.md 规范冲突检测条款**：这是必须停下裁决项。两条路 —— (A) 把规则改成「extern 需按包登记审计」（贴合现实），统一 net/threading/compression/crypto/test/time/io.binary 全部登记进审计表；(B) 大量 native sink 收编回 core/io（不现实）。裁决后同步 `README.md` 审计表 + `overview.md` + `organization.md` 三处。

---

## 6. 文件 / 类型行数硬限违规（code-organization.md：文件 500 行 / 类型 200 行）

`src/` 下（不含 tests/bench）超 500 行硬限文件：

| 行数 | 文件 | 备注 |
|------|------|------|
| 2198 | `z42.numerics/src/BigInt.z42` | 类型硬限 10×；拆核心算术 / bitops / 素性检测(MR+BPSW ~700行) / mag helpers |
| 1647 | `z42.yaml/src/YamlParser.z42` | 单类 ~1600 超类型硬限 8× |
| 1406 | `z42.net/src/Http/HttpClient.z42` | 拆 ConnectionPool/DigestAuth/ResponseReader/Strings |
| 1102 | `z42.crypto/src/Aes.z42` | 拆 AesCore/Ctr/Cbc/Gcm/Ccm/Tables |
| 931 | `z42.toml/src/TomlParser.z42` | |
| 770 | `z42.uri/src/Uri.z42` | 拆 Uri/UriParser/UriCodec 三文件 |
| 729 | `z42.cli/src/ArgParser.z42` | |
| 719 | `z42.time/src/DateTime.z42` | DateTimeOffset 挤此文件是 TypeChecker E0402 workaround，修后拆 |
| 566 | `z42.crypto/src/Rsa.z42` | |
| 551 | `z42.net/src/WebSockets/WebSocketClient.z42` | |
| 532 | `z42.regex/src/Regex.z42` | |
| 514 | `z42.compression/src/Tar.z42` | 随 z42.archive 拆分解决 |

另超 300 软限：JsonParser 453 / YamlValue 406 / RegexParser 382 / Assert 365 / String 339 / MulticastPredicate 376 / MulticastAction 303 / MulticastFunc 302 / Random 259。多数隐含类型 200 行硬限违规。
> 每项为独立 refactor change，单独 commit，不与功能变更混合。

---

## 7. 横切一致性与文档

### 7.1 README 大面积失真（docs change）
- **顶层 README.md 库列表只 17 项，实际 24 目录**：漏 z42.net/yaml/numerics/threading/compression/build/project；「已规划未启动」表 7 项（diagnostics/net/json/crypto/compression/numerics/threading）已落地，其中 3 个同时出现在上方库列表 → 文档内部自相矛盾；真正未启动只剩 z42.async/z42.linq。
- **行级过时**：io.binary namespace 说 `Std.Binary` 实为 `Std.IO.Binary`；z42.text 说含 Regex（已独立成 z42.regex）；collections/io/encoding/time/crypto 描述严重缩水；「既有包扩展计划」多项已完成未勾（io 的 Stream 族、math 的 Random→z42.random 等）。workspace.toml 头注释仍写「6 个 stdlib 包」实际 22。
- **各包 README 与 src 脱节**：crypto 最严重（README 只列 4 算法，实 23 文件，且「不做 CSPRNG」职责描述与 `SecureRandom.z42` 直接矛盾）；net README 仍写「K1: TCP only」；core/src/README.md 漏 30+ 文件（Array/Attribute/Platform/Reflection 等）；yaml README 无文件索引违反六段制。
- **z42.build / z42.project 顶层 README 零痕迹**：两个 parked「接口先行」库（namespace `Z42.*` 非 `Std.*`，无 *.z42.toml、不入 workspace、0 测试），至少须在 README 列出并标 Parked。

### 7.2 命名空间遗留点
- `Std.Archive` 寄居 z42.compression（应随 §1.6 拆包）；`Std.Uri.Uri`、`Std.Regex.Regex` 命名空间与类型同名 stutter；`Z42.Build` 非 `Std.*` 约定只写在 workspace.toml 注释无正式说明；异常统一放平铺 `Std` 造成「类型在 Std.X、异常在 Std」割裂且无文档写明。

### 7.3 异常摆放三风格并存
`Exceptions/` 子目录（core/io/net 部分）、根级单文件（CliException/TomlException/UriException）、合并一文件（`Compression/Exceptions.z42` 装两类）；net 异常散在 `Exceptions/`+`Http/`+`WebSockets/` 三处；z42.threading 两异常根并列（ThreadException 与 ChannelDisconnectedException 不同根）。

### 7.4 测试覆盖不均
| 包 | src 行 | 测试文件 | 评注 |
|----|--------|---------|------|
| z42.build | 360 | 0 | parked，接入编译时须补 GREEN |
| z42.project | 669 | 0 | 同上 |
| z42.core | 3716 | 7 | 最基础、覆盖最薄（531 行/文件）|
| z42.uri | 783 | 6 | 770 行 parser 仅 6 用例 |
| z42.time | 966 | 7 | 719 行超限文件覆盖薄 |
| z42.toml | 1432 | 8 | 931 行 parser |
（健康对照：crypto 6830/28、net 6748/50、io 3002/46、collections 532/19）
优先补：core（地基）、uri/time/toml（超限大文件 + 个位数用例）。

---

## 8. 建议的推进路线（分批）

**批次 A — 高危 fix（低成本、独立提交）**
1. Mutex/RwLock try/finally（死锁）
2. Queue/Stack 空检查（状态损坏）
3. Dictionary.FindSlot 哈希取负溢出、BigInt.Equals 类型检查、Assert null 判空、Substring 越界
4. crypto 安全 fix：ECDSA/Ed25519 固定迭代 ladder、CBC padding 常数时间、ConstantTimeEquals 公开 API、AEAD 随机 nonce 入口
5. HTTP TE/Content-Length 走私、chunk-size long、WebSocket 分片/UTF-8 校验
6. 三格式 parser 深度上限；JsonParser 大整数 catch 回退

**批次 B — 性能（查找表 static 化 + O(n²) 消除）**
1. crypto 全线常量表提 static（AES S-box / SHA/BLAKE/Keccak / Zip CRC）
2. `ToCharArray()` + O(1) char-index perf spec（解全库字符串 O(n²)）
3. BigInt Horner 读入 / chunk 输出 / 9 位组 Parse；值树对象键索引；HTTP body 分段 buffer

**批次 C — 机制扩展（解锁重复消除，多为 lang/编译器前置）**
1. foreach → IEnumerator 路径（解锁集合层）
2. IDisposable 落到 Stream 族 + using
3. HashAlgorithm / IAead / WeierstrassCurve 抽象（crypto ~2000 行去重）
4. 泛型 delegate（Multicast 去重）、泛型静态方法（Random.Shuffle）
5. 编译器 typed-overload-resolution（消一片 API 疤痕）

**批次 D — refactor / 结构（独立 commit）**
1. 12 个超限文件拆分（BigInt / YamlParser / HttpClient / Aes 优先）
2. 抽 z42.data 共享 Value 树 + z42.text TextCursor + escape 基建
3. 拆 z42.archive；numerics→random / threading→diagnostics 断依赖边
4. Exception.ToString 基类化、Base32 参数化核心

**批次 E — 文档 + 裁决**
1. **extern 规则冲突裁决**（§5，先于相关动手）
2. 顶层 + 各包 README 全面刷新；异常摆放 / 命名空间约定成文
3. 补 core/uri/time/toml/build/project 测试

---

> 生成方式：5 个并行审查 agent 分组通读全部 24 包 src 源码（合计约 3.5 万行）+ 横切核对，
> 结果去重合并。每条均可追溯到具体 `文件:行`，落实时按 workflow.md 各自建 change。
