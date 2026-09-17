# 批 4 核实记录（reference/stdlib）

> 与 [batch3-verification.md](batch3-verification.md) 同体例：记录**设计页断言 vs 源码事实**的逐条核对
> 结果，以及核实过程中实跑挖出的**实现缺口**。缺口部分是后续单开 change 的依据。

## 方法

沿用批 3 的方法论并补强一步：

1. **源码是唯一真相来源**——`grep -n "public" src/libraries/<pkg>/src/*.z42` 拿全公开面，
   再逐个打开确认签名（参数名 / 参数类型 / 返回类型 / static / override）。
   **不照抄设计页的「API surface」代码块**（实测这些块系统性漏项）。

   > ⚠️ **grep 命中 ≠ 事实**：本批协调方用 `grep -rl "extern\|\[Native" src/libraries/<pkg>/src`
   > 数出「io 3 个文件 / net 4 个 / json 1 个仍在声明 extern」，并把它当线索下发。
   > 核实后**全部落空**——那些命中是**注释**（「本文件不再自带 extern，已上移 core」），
   > 四个包的真实声明数是 **0**。**grep 到的行必须打开看，尤其是注释密集的文件。**
2. **实跑探针**（`./.z42/z42 run`，单文件顶层 `void Main()`）核实返回值形态、边界行为、异常类型。
3. 批 3 总结的三条机械筛选规则全程适用（Phase 表默认过期 / `.cs` 路径表作废 /
   Exxxx 断言必须给发射点）。

## 落空率

设计页里凡「API surface」代码块，**无一完整**。典型漏项幅度：

| 包 | 设计页声称的公开面 | 实际公开面 |
|---|---|---|
| `z42.diagnostics` | `LogLevel` + `Log` 两个类型 | 外加 `Heap` / `Retainer` / `RootRef` / `RootKind` / `RuntimeStats` / `RuntimeCounters` 六个——**少了三分之二** |
| `z42.cli` | `ArgParser` 6 方法 / `ParseResult` 5 方法 | 12 / 13，外加 `SubcommandRouter` / `SubcommandMatch` / `CommandResolution` 整条子命令线 |
| `z42.uri` | `Uri` 的 accessor + 编解码 | 外加 `IsIPv6Literal` / `GetHostName` / `EffectivePort` / `DefaultPortFor` + 8 参数公开构造器 |
| `z42.crypto` | v0 摘要 + HMAC | 外加 `Pbkdf2` / `HkdfSha1` / 三个 `DeriveHex` / `ConstantTime` |
| `z42.random` | （根本没有 API 清单） | 3 构造器 + 14 方法 |
| `z42.encoding` | 缺 `EncodingSingletons`、`Encoding(int)` 构造器 | 11 个公开类型 |

### 「文档声称的语言限制」全部已不成立

设计页常把当时的编译器缺陷写成永久限制，实测**三条全废**：

| 设计页断言 | 实测 |
|---|---|
| 「z42 lexer 暂不支持 hex 字面量 `0xFFFFFFFF`」 | 早已支持（`0xFFFF` = 65535；`Aes.z42` 全篇在用） |
| 「primitive `int[]` 不零初始化，`new int[N]` 后是 Null」 | 已零初始化 |
| 「`(int)long` cast 不截断」 | 已截断 |

⇒ 沿用批 3 的规则：**设计页里的「限制表」默认判过期**。

### 自相矛盾（同一份文档内部打架）

| 文档 | 矛盾 |
|---|---|
| `design/stdlib/crypto.md` | 正文列了 AES-GCM / CCM 已实现，同一条目末尾又写「**Out of scope (deferred)**: GCM / AEAD」 |
| `design/stdlib/cli.md` | Deferred 段说「Required mark 靠 caller 自己 check `== ""`」，而 `AddRequiredOption` + `WasOptionSet` 早已存在 |
| `design/stdlib/organization.md` | 第 41 行把 io / net / threading 列为「零 interop 纯脚本」，第 177 行又写 threading 是「✅ VM intrinsic」 |
| `design/stdlib/time.md` | 「不含日历分解 getter / ISO 解析 / `DateTimeOffset`」——三者全部存在 |

### `z42.time` 包已删所引发的连锁错误

`time.md` 整页按「`z42.time` 独立包」写，而该包已不存在（类型在 `z42.core/src/Time/`，
命名空间仍是 `Std.Time`）。连带错误：

- 「z42.time 直接使用 VM intrinsic，不通过 z42.core 中转」——**方向正相反**：
  `__time_now_ms` / `__time_now_mono_ns` 的唯一声明点是 core 的 `Std.Runtime.Clock`
  （`Clock.z42:12-17`），`DateTime` / `Stopwatch` 都经它（`DateTime.z42:15`、`Stopwatch.z42:13`）
- `design/stdlib/random.md`、`diagnostics.md` 都声称「依赖 `z42.time`」——两者实际只依赖 `z42.core`

### 照文档写代码会编不过 / 会写错

| 文档 | 断言 | 事实 | 后果 |
|---|---|---|---|
| `design/stdlib/diagnostics.md`（两处，含「实施期发现」） | `LogLevel` 常量是 `TRACE`/`DEBUG`/`INFO`/`WARN`/`ERROR` | PascalCase 的 `Trace`/`Debug`/`Info`/`Warn`/`Error`（`LogLevel.z42:8-12`） | 照抄**编不过** |
| `design/language/string-builtins.md` | **「`Length` 单位：UTF-8 字节数（与 Go `len(s)` 一致）」** | `Length` 是 **Unicode 标量个数**（O(n)）；UTF-8 字节数是另一个属性 `ByteLength`（O(1)）。实测 `"你好".Length == 2` / `ByteLength == 6`（`String.z42:44,51`） | 照抄**索引代码全错**，且编得过 |
| `design/stdlib/json.md` | JsonPath「covering `.`, `[i]`, `*`, recursive descent」 | `*` **显式抛异常**，`..` 报 `empty segment after '.'` | 照抄运行期炸 |
| `docs/book/src/stdlib/json-serde.md` | 「字段与属性均生效（**含计算属性**）」 | 计算属性上的 `[JsonProperty]`/`[JsonIgnore]` **都不生效**；同一页的「实现原理」节自己写的是「不支持」——**单页自相矛盾** | 以为能排除，实际排不掉 |
| `src/libraries/z42.yaml/README.md` | 示例写 `Yaml.Parse(text)` / `Yaml.Stringify(v)` | **根本没有 `Yaml` 这个类**（只有 `YamlValue`）；同一 README 后半段的例子又写对了 | 照抄编不过 |
| `src/libraries/z42.text/README.md:12` | `StringBuilder.Length { get; set; }` | **没有 setter**；源码注释自己也写明「C# `Length` setter is NOT provided」（`StringBuilder.z42:91-93`） | 照抄编不过 |

### 零发射点的错误码（沿用批 3 的第 3 条筛选规则）

`string-builtins.md` 声称方法名拼写错误报 `Z0401`——`Z0401` 是**零发射点死码**
（错误码前缀早已是 `E0xxx`）。实测 `s[i]` 报的是 **E0402**（`index on non-array 'String'`）。已删。

---

## 附录 A · 实现缺口（值得单开 change）

### A0 · 最高优先：单文件模式用不了跨包命名空间的后半边

**四个组（C / D / G / H）在互不知情的情况下独立撞到同一条，根因已定位到代码。**
直接影响手册的单文件教学风格。

现象：`z42 run <单文件>.z42` 里，**编译期全过，运行期才炸**——

| 写法 | 运行期错误 |
|---|---|
| `Thread.Sleep(5)` | `MissingSymbolException: undefined function Std.Threading.Thread.Sleep$1$i64` |
| `new Mutex<long>(0)` | `type Std.Threading.Mutex could not be resolved` |
| `new Stack<int>()` | `type 'Std.Collections.Stack' could not be resolved` |
| `new MemoryStream()` / `new FileStream(...)` | `type Std.IO.MemoryStream could not be resolved`（而同在 `Std.IO` 的 `Console` / `File` 因为住在 core，单文件可用） |
| `using Std.Net.Sockets;` + `new TcpListener()` | `type Std.Net.Sockets.TcpListener could not be resolved` —— 但**只要额外写一行 `using Std.Net.Http;` 就立刻可用**；`Std.Net.Http` / `Std.Net.WebSockets` 单独用都正常 |
| `using Std.IO` 下的 `FileMode` | 编译期 E0401（这半边连编译都过不了） |

根因链：

1. `Std.Threading` / `Std.Collections` / `Std.IO` 三个命名空间**跨两个包**
   （core 一半 + 能力库一半），例如 `z42.core/src/Native/ThreadingNative.z42:11`
   也声明 `namespace Std.Threading`
2. `DepScan` 的 nsMap 是 **prelude-first + first-wins**
   （`src/compiler/z42c.pipeline/src/DepScan.z42:22,105-120`）⇒ `Std.Threading → z42.core.zpkg`，
   DEPS 里根本没有 `z42.threading.zpkg`
3. 既有补救是把 toml **声明的**依赖并进 DEPS
   （`src/compiler/z42c.pipeline/src/PackageCompile.z42:311-317`，注释即 `fix-crosspkg-ns-split-deps`）
4. 但**单文件合成的清单没有 `[dependencies]` 段**
   （`src/toolchain/launcher/core/launcher.z42:267-293`）⇒ 补救不生效

修法方向（二选一）：合成清单时把已安装的 stdlib 包全量写进 `[dependencies]`；
或让单文件路径按 `using` 取 nsMap 的**全部**提供包而非 first-wins。

> 附带：`z42.collections` 那条的运行期错误信息说「loaded package may be older」，**误导**——
> 与版本无关，是依赖根本没进 DEPS。

### A0b · 次高优先：native 压缩的所有错误路径自死锁 → VM 永久挂起

`src/runtime/src/native/ext.rs` 的每个 `wrap_*` 函数开头
`let guard = LOADED_COMPRESSION.lock();` 一直持有到函数结束；而 `rc != 0` 时走
`bail!("{}: {} …", last_error_string(), rc)`，`last_error_string()`（`ext.rs:476`）
**再锁同一把不可重入的 `parking_lot::Mutex`** ⇒ 自死锁。

**11 个发射点**：`ext.rs:511,533,554,574,595,615,636,656,676,698,717`。

最小复现（两个都 **0% CPU 挂死**，不是死循环）：

```z42
Gzip.Decompress(Utf8.GetBytes("not-compressed-data-at-all"));  // 损坏输入
Gzip.Compress(data, 0);                                         // level 越界（合法区间 1..9）
```

`sample` 抓到的栈：`wrap_deflate_decompress` → `last_error_string` → `parking_lot::RawMutex::lock_slow`。

**影响面 = 六种算法（Gzip/Zlib/Deflate/Zstd/Brotli/Lz4）的全部失败路径**，
含 HTTP 收到坏 `Content-Encoding` 时。与 [[z42-blocking-native-gc-deadlock]] 同族。

> 配套：`Std.CompressionException` 是**零发射点死类**（只有 `Brotli.z42:34` / `Lz4.z42:36`
> 两条注释提它）。修这条时应一并决定错误是否包装成该类型。

### A0c · `WebSocketServer` 收不到任何合规客户端的消息（零测试覆盖）

`_FrameCodec.ReadFrame`（`_FrameCodec.z42:195`）是按**客户端视角**写的：对任何带掩码的帧直接抛
`WebSocketProtocolException: server frame is masked (RFC 6455 §5.1 violation)`。
而 RFC 6455 **§5.3 要求 client→server 的帧必须带掩码**。
`WebSocketConnection.Receive()` 复用了它 ⇒ **z42 自己的 `WebSocketClient` 连上 z42 的
`WebSocketServer` 后，第一条消息就炸**（客户端侧表现为永久挂起）。

`tests/websocket_server.z42` 顶部注释明确写着「e2e 不测，会重复那套已知 flaky 的线程测试基建」
⇒ **这条路径零覆盖**，所以一直没被发现。

修法：`ReadFrame` 需要一个「期望对端是否掩码」的方向参数 + 掩码解码分支。

### A0d · regex 的三类静默语义缺口

与 [[z42-silent-semantic-gaps]] 同族，**pattern 会安静地匹配错东西**：

| 写法 | 期望 | 实际 |
|---|---|---|
| `[\d]` `[\w]` `[\s]` | 类内转义类 | **退化成字面 `d` / `w` / `s`**。`[\d]+` 匹配 `"ddd"`，不匹配 `"123"` |
| `\1` `\x41` `A` `\A` `\z` `\Z` `[\b]` | 反向引用 / 数值转义 / 锚 | **全部静默退化成字面量**，不报错 |
| `(a\|ab)c` `(?:a\|ab)c` `(a*)ab` | 回溯进组内重试 | **NO MATCH**——任何 group 一旦整体匹配成功就不再回溯进组内 |

第三类最难自查：同层的 `\d+5` / `a+ab` / `ab\|abc` **都正常**，只有跨 group 边界才坏。
设计页把它描述成「alternation 的一个特例」，实测**交替与量词都中招，非捕获组同样**
（`Regex.z42:396-413`，GROUP 失败路径不重试 child）。

前两类至少应在 parser 里改成编译报错（「不支持，别静默当字面量」）；
第三类是引擎结构性问题，最小修法是 GROUP 失败时把控制权交还给 child 继续找下一个可行匹配。

### A1 · 崩溃与不可捕获的失败

| # | 缺口 | 证据 |
|---|---|---|
| 1 | **`SubcommandRouter.Match` 撞嵌套子命令崩 VM**。`AddRouter("build", …)` 后 `root.Match(["build","package"])` → `_parsers[i]` 为 `null` → `VCall: expected object, got Null`，**`catch (Exception)` 拦不住，进程直接死** | `SubcommandRouter.z42:131` |
| 2 | 同上第二面：**空引用调用没有走可捕获的异常路径**——任何 `null` 方法调用都是不可 catch 的 VM abort，不只这一处 | 同上 |
| 3 | **`List<T>` 索引越界越过容量时是 VM abort，不是可 catch 的异常** | `src/runtime/src/interp/exec_array.rs:196` |
| 3e | 🔴 **`Environment.SetEnvironmentVariable` 非法名 abort 整个 VM**：`SetEnvironmentVariable("BAD=NAME", "x")` → Rust `std::env` panic → `fatal runtime error: failed to initiate panic` → SIGABRT，**catch 不住**。应在 native 侧校验名字（无 `=`、非空、无 NUL）并返回可捕获异常 | 实测 |

### A2 · 静默错误 / 静默数据损坏

| # | 缺口 | 证据 |
|---|---|---|
| 3b | 🔴 **`foreach` 直接吃 native builtin 返回的 `char[]`，每个元素都是 `null`**。`foreach (var c in s.ToCharArray())` 跑满两轮但 `c` 全 `null`；先 `char[] a = s.ToCharArray();` 再 foreach 就正常。已 narrow：用户自定义返回 `char[]` 的方法内联进 foreach **正常**，脚本方法 `Split(string)` 内联**正常**，`"ab".ToCharArray()[0]` 索引**正常**——只有「foreach 直接吃 native builtin 的 `char[]` 返回值」坏。元素个数对、值全丢 | 实测 |
| 3c | **对 `Stack`/`Queue`/`LinkedList`/`SortedSet`/`PriorityQueue`/`HashSet` 写 `foreach` 能通过编译**，运行期吐一屏 `ArrayLen: expected array, got Object(ScriptObject { type_desc: ... })` 的 Rust 内部 dump。这些类型既无 `GetEnumerator()` 也无索引器，应在编译期报 E04xx | 实测 |
| 3d | **`string.Split(char)` 直接崩**：``VCall: expected object, got Char(',')`` at `Std.String.Split (line 16)` | 实测 |
| 4 | **`List<T>` 索引器不按 `Count` 校界**：`i ∈ [Count, capacity)` 时**静默返回 `default(T)`**。`new List<int>(8)` 里放 2 个元素后 `l[5]` → `0`，不报错 | 实测 |
| 5 | **`Count` 是 `public` 可写字段**（`List` / `Dictionary` / `HashSet` / `ReadOnlyCollection` 全部）。`l.Count = 1;` 编译通过并直接改长度 | `List.z42` 等 |
| 6 | **`Encoding(int kind)` 是死参数**：公开构造器收任意 `kind`，永远走 UTF-8。`new Encoding(999)` 静默当 UTF-8 | `Encoding.z42:25` |
| 7 | **`ArgParser` 不做重名检测**：同一 long/short 注册两次静默接受，后者永远取不到值 | 实测 |
| 8 | **`X25519.U_BASE` 是可写 `public static` 字段**（不是 `const`）：写入后污染同进程后续所有 `ScalarMultBase` | `X25519.z42:36` |

### A2b · 数据格式包的静默错误

| # | 缺口 | 证据 |
|---|---|---|
| 28 | 🔴 **YAML 不支持 `- key: v` 之后的多行字段**：`- name: bob` 换行再写 `age: 3` 报 `unexpected indent at sequence level`。这是 Docker Compose / K8s / GitHub Actions 的主流写法，**真实配置基本一进来就炸**。本批影响面最大的缺口 | `YamlParser.z42:465-470`（注释自称 "v1 case"） |
| 29 | **TOML `NaN` 序列化产出非法 TOML `NaN.0`**：`FormatValue` 的「补 `.0`」判定只挡了含 `n`/`i` 的小写拼写，挡不住 `NaN`；再解析必失败。`inf`/`-inf` 正常 | `TomlWriter.z42` |
| 30 | **JSON `Stringify` 对 NaN/Infinity 输出非法 JSON**（`NaN` / `inf` / `-inf`），且 `ParseRelaxed` 拼写不一致、**读不回自己写出的形态** | `JsonWriter.z42:26` |
| 31 | **JSON serde 顶层形状不匹配静默吞掉**：`[1,2]` 反序列化成某个类**不报错**，返回成员全默认的对象 | `JsonBinder.z42:104` |
| 32 | **计算属性上的 `[JsonIgnore]` 无效**——不只是改不了键名，而是**排不掉**，计算属性一定会被写进 JSON | `JsonMember.z42:8` |
| 33 | **YAML flow 上下文里的 `&anchor` 声明被静默当普通标量**：`a: [&x 1, *x]` 报 `undefined anchor: *x`，错误指向无辜的一方 | 实测 |
| 34 | JSON `42.0` → `Stringify` 输出 `42`，再解析 kind 变 Long（float-ness 丢失）；大整数溢出静默降级 double 后输出 `100000000000000000000` | 实测 |

### A2c · 反射面

| # | 缺口 | 证据 |
|---|---|---|
| 35 | **`Type.GetFields()` 返回 private / protected 字段**，与源码注释「Public instance fields」和 C# 默认（只 public）都不符。当前反射可读写任意私有字段（`FieldInfo.GetValue/SetValue` 实测能改私有槽） | `Type.z42:183` vs 实测 |
| 36 | **`__` 前缀的 VM 存储槽是 `public`，污染反射结果**：`Std.Type` / `FieldInfo` / `MethodInfo` / `PropertyInfo` / `ParameterInfo` 上的 `__name` / `__fullName` / `__typeArgs` / `__qualified` / `__attrCache` / `__elementName` / `__asmId`。已有 `__prop_*` 被过滤的先例 | 实测 |
| 37 | **`Assembly.GetTypes()` 对 root assembly 返回空数组**：主程序与 stdlib 的类型全在 root 上下文 ⇒ 这个 API 对绝大多数程序拿不到任何东西 | 实测 |
| 38 | `ParameterInfo.DefaultValue` 折不出 enum 成员与命名常量（给 `null`），而 `IsOptional` 仍 `true` ⇒ **`DefaultValue == null` 不等于没有默认值** | 实测 |

### A2d · 集合的 GC 保留与封装

| # | 缺口 | 证据 |
|---|---|---|
| 39 | **一批本该 private 的成员是 `public`**：`List.Grow` / `Dictionary.Grow` / `Dictionary.FindSlot` / `Queue.Grow` / `Stack.Grow` / `SortedSet.Grow`，以及 `LinkedListNode.SetNext` / `SetPrevious`（外部调用会破坏链表不变式） | 源码 |
| 40 | **GC 保留不一致**：`List.RemoveAll` 不清尾部槽；`HashSet.Clear` / `Stack.Clear` / `Queue.Clear` / `PriorityQueue.Clear` / `SortedSet.Clear` 只把 `count` 归零、不清元素引用——**与同文件里 `List.Clear` / `RemoveAt` / `Dictionary.Remove` 特意清引用的做法自相矛盾**（那几处注释明写是为了不让被删对象被 GC 视为可达） | 源码 |

### A2g · 归档条目名的静默破坏

| # | 缺口 | 证据 |
|---|---|---|
| 48 | 🔴 **`Tar.Write` 静默截断 / 破坏条目名**：`_WriteStr`（`Tar.z42:470`）超过 100 字节直接截断，字符按 `(byte)s.CharAt(i)` 写（Latin-1 截断），写入路径**从不使用 ustar 的 155 字节 prefix 字段**。实测 120 字符名 → 100 字符；`中文.txt` → `-.txt`。**无任何报错** | 实测 |
| 49 | 🔴 **`Zip` 条目名往返破坏**：`Zip.Write` 按 UTF-8 编码并置 GPBF bit 11，但 `Zip._ReadStr`（`Zip.z42:405` 附近）按 `(char)bytes[i]` 单字节解码。实测 `中文.txt` → `ä¸­æ.txt` | 实测 |
| 50 | **`MemoryStream.Close()` 是空方法**（`MemoryStream.z42:139-146` 全是注释），注释却写着「drop the buffer ref」。关闭后流仍完全可读可写 | 实测 |
| 51 | **`BinaryReader.ReadBytes` 的 EOF 包装是死 catch**：`BinaryReader.z42:121` catch `InvalidOperationException`，而 `Stream.ReadExactly` 现在抛 `EndOfStreamException`（`Stream.z42:172`）⇒ 同一个类两种错误类型（`ReadByte` 抛 `BinaryException`，其余抛 `EndOfStreamException`） | 实测 |
| 52 | **`Std.IOException` 类型不存在**，但 `FileStream.z42:22` / `FileMode.z42` 注释与设计文档多处声称文件失败抛它；实际全是基类 `Std.Exception`，调用方只能按 message 区分 | 实测 |
| 53 | **`_GrowBuf` 泄进公开命名空间**：`Zip.z42:424` 是 `public class _GrowBuf`，处在 `Std.Archive` 下，用户可见 | 源码 |
| 54 | **四个文本读写器不继承基类**：`StringReader` / `StreamReader` / `StringWriter` / `StreamWriter` **一个都不继承** `TextReader` / `TextWriter`（全仓唯一子类在测试文件里）⇒ 无法按基类多态使用 | `grep ": TextReader"` |

### A2e · JSON serde 的静默数据丢失

| # | 缺口 | 证据 |
|---|---|---|
| 41 | 🔴 **`float` 反序列化静默丢值**：`{"F":2.5}` → `F == 0`。`FromJson` 按声明类型 `Std.Single` 分派，一个分支都不命中，落 `_fromObject` | 实测 |
| 42 | 🔴 **`char` 序列化成 `{}`**——静默数据丢失 | 实测 |
| 43 | 🔴 **`enum` 反序列化产出畸形对象 + 延迟爆炸**：造出空 `ScriptObject`，**下一次读它**才抛 `__box_prim: expected integer value, got Object(...)`，且异常文本把整个 `TypeDesc` 的 Rust `Debug` 串吐给用户。正向序列化输出序号 `1`，看起来「支持」——不对称 | 实测 |
| 44 | **必填 ctor 参数缺键塞 `null`**：`FromJson(pt, JsonValue.OfNull())` 对值类型参数也返回 null，不报错 | 实测 |

### A2f · native 表面的计账失真与死条目

| # | 缺口 | 证据 |
|---|---|---|
| 45 | 🔴 **`src/libraries/README.md` 的「Extern 现状审计表」失真 8 倍**：自称「当前总计 ~34」，实测 VM 注册表 **321 条**、stdlib 声明的不同符号 **288 个**。该表被 CLAUDE 规范指定为「每次 stdlib 改动起手必看」的 SoT——**一个失真 8 倍的 SoT 比没有更糟**。表里还把已删的 `z42.math` / `z42.time` 当现存 | `builtin_table.rs` + `builtin_table_ext.rs` |
| 46 | **19 个死 builtin**：`__mutex_*` / `__rwlock_*` / `__channel_*` 在 VM 表里但**无人声明**（`Mutex`/`RwLock`/`Channel` 早已是纯 z42，只用 `MonitorNative`）。下标即 `BuiltinId` 且烤进 zbc，删不掉，只能留着占位 | VM 表 vs 声明集 |
| 47 | **`[Native]` 的 intrinsic 名编译期零校验**：`[Native("__definitely_not_a_builtin")]` 顺利编过，调用时才 `MissingSymbolException`。设计页声称「不存在时报编译期错误」——**不成立** | 实测 |

### A2h · 文件 / 进程 / 控制台

| # | 缺口 | 证据 |
|---|---|---|
| 55 | 🔴 **`Console.Write` / `WriteLine` 不调用用户的 `ToString()` 覆写**，打成 `TypeName{...}`；`"" + obj` 也不调；**只有字符串插值 `$"{obj}"` 会调**。同一个值三种写法两种结果 | 实测 |
| 56 | 🔴 **`Console.ReadLine()` 无法表达 EOF**：EOF 与空行都返回 `""`，返回类型是非 nullable `string`（`io.rs:221-231`），stdin 关闭后**无限返回 `""`** ⇒ 任何逐行读 stdin 的脚本都是死循环隐患 | 实测 |
| 57 | 🔴 **`.StdinBytes()` 在非 Pipe 模式下静默丢数据**：默认 stdin 是 `Null`，`process.rs:298-305` 的 `if let Some(mut sin) = child.stdin.take()` 直接跳过 ⇒ `cat` 拿到空输入、exit 0、无任何警告 | 实测 |
| 58 | **`Stdio.ToFile(path)` 用在 stdin 上必定失败**：`Process.z42:134-139` 只对 stdout/stderr 传 `GetPath()`，stdin 那一路恒传 `None` ⇒ `process.rs:143` 抛 `stdin Stdio.ToFile missing path` | 源码 |
| 59 | **`WriteStdin` 对非 pipe 句柄误杀整个句柄**：`ProcessHandle.CheckHandleResponse` 把「writer 是 None」和「slot 不存在」混同，统一置 `_disposed = true` ⇒ 一次误用后连 `Wait()` 都拿不到退出码。`process.rs:620-623` 应区分 `None` 与 `Some(None)` | 源码 |
| 60 | **`File.Exists(dir)` 返回 `true`**（native 走 `exists()` 不是 `is_file()`），与 .NET BCL 语义相反。判「是普通文件」要写 `File.Exists(p) && !Directory.Exists(p)` | 实测 |
| 61 | **同包内两套解码口径**：`File.ReadAllText` 对非 UTF-8 **严格抛错**，而 `ProcessResult.Stdout` 是 **lossy 解码** | 实测 |
| 62 | **`Kill()` 与 `KillForce()` 行为完全相同**：`process.rs:584-591` 明写 `let _force` 是 no-op，两者都 SIGKILL | 源码 |
| 63 | **`Ansi.SetEnabled` 不可撤销**：调过之后自动检测对本进程永久失效，传 `false` 也回不去 | 实测 |
| 64 | **`File.Copy` / `File.Move` 静默覆盖**已存在的 dst | 实测 |
| 65 | **glob 的 `*` 跨 `/`**：`GlobRecursive(root, "*/x.txt")` 匹配到 `sub/deeper/x.txt`，与 `Path.z42:258` 注释「`*` matches any (except `/`…)」相反 | 实测 |

### A3 · 安全性

| # | 缺口 | 证据 |
|---|---|---|
| 9 | **RSA-OAEP padding oracle**：解封失败抛出**四条互相可区分**的消息，与 RFC 8017 §7.1.2「不可区分」要求正相反 | `Rsa.z42:405/435/446/452` |
| 10 | **`Sha512` 泄漏 7 个内部函数为 `public`**（`_initialHash` / `_pad` / `_compressBlock` / `_lshr64` / `_rotr64` / `_readBE64` / `_writeBE64`）。z42 有 `internal`，这些应是 `internal` | `Sha512.z42:52-224` |

### A4 · 并发（`z42.threading`）

| # | 缺口 | 证据 |
|---|---|---|
| 11 | **会合通道（`capacity == 0`）上已阻塞的 `Send` 不会被 `Close()` 唤醒**：那个值必须有人取走 `Send` 才返回，否则该线程永久挂起 | 实测 |
| 12 | **没人 `Join` 的 worker 异常被静默吞掉**，且无全局「未捕获异常」钩子 | 实测 |
| 13 | **跨线程异常不保真**：只带 `Message` 字符串，原类型与栈丢失 | 实测 |
| 14 | `Mutex<T>` 没有 `TryLock`（`RwLock<T>` 有 `TryRead` / `TryWrite`），重入抛的是基类 `Exception` 而非专用类型 | 实测 |

### A5 · 编译器 / 类型推断

| # | 缺口 | 证据 |
|---|---|---|
| 15 | **`Predicate<T>` / `Func<T,T,int>` 形参的 lambda 必须显式标注参数类型**，否则参数以未实例化的 `T` 参与类型检查并报 E0402。`list.Find(v => v % 2 == 0)` 编不过 | 实测 |
| 16 | **重载决议无法在 `Parse(string)` 与 `Parse(Stream)` 之间可靠选择**——`z42.toml` 因此被迫把流式入口命名为 `ParseStream` 而非重载 | `TomlValue.z42` |

### A6 · API 不一致 / 缺失

| # | 缺口 | 证据 |
|---|---|---|
| 17 | `Random` 抽样 API 不对称：有 `ShuffleInt/Long/String`，只有 `SampleInt/String`——**缺 `SampleLong`** | `Random.z42` |
| 18 | `Std.Encoding.Utf32` 只有 `GetBytesLE/BE` + `GetStringLE/BE`，**没有**不带后缀的形态（与 `Utf8` / `Utf16` 不一致） | `Utf32.z42:17-58` |
| 19 | `UriParser` 实例不可复用：第二次 `ParseUri()` 抛 `empty URI`（内部游标不重置），但它是 `public` | `UriParser.z42` |
| 20 | `Uri.DecodeComponent` 把**任何位置**的裸 `+` 解成空格（form-urlencoded 习惯），对 path / fragment 解码是静默语义污染 | `UriCodec.z42:70-73` |
| 21 | `DateTime.ToString()` 返回**裸 Unix 毫秒串**而非 ISO-8601 | 实测 |
| 22 | `DateTime` / `TimeSpan` **无运算符重载**：`a < b` 报 E0402 | 实测 |
| 23 | `ArgParser`：`--` 不是选项终止符（`Parse(["--","build"])` → `unknown option '--'`）；负数不能当 positional | 实测 |
| 24 | env 后备命中时 `WasOptionSet` 仍为 `false`——调用方无法区分「env 提供了」与「用了默认值」 | 实测 |

### A7 · 源码内注释自身过期

| # | 位置 | 内容 |
|---|---|---|
| 25 | `ArgParser.z42:196-201` vs `:347-356` | docstring 自称用 `_optionDefaults[i] == ""` 哨兵校验必填，实际 `Parse` 用的是 `_optionWasSet` |
| 26 | `Uri.z42:12` vs `:180` | 文件头注释写「不做相对 URI 解析（resolve），future work」，而 `Resolve` 就在同文件 |
| 27 | `src/libraries/z42.crypto/README.md` 首段 | 写「本包**不**做需要 OS 熵源的 CSPRNG」，而 `SecureRandom` 就在本包；`核心文件`表列 5 个文件，实际 24 个 |

---

## 附录 B · 本批遗留的裁决项

1. **`README-template.md` 与六段制冲突**：`docs/agent/rules/readme-writing.md` §七 自称「六段模板
   （唯一 SoT，其他文件只链接不复制）」，而 `design/stdlib/README-template.md` 是另一套分段
   （职责 / src 核心文件 / 入口点 / 依赖关系 / Deferred / 测试）。实际 stdlib 包 README **两个都不完全遵守**，
   更接近后者。按「移入 agent/rules」直接搬 = 制造第二份 SoT。**本批未动该文件，待裁决。**
2. **`_` 前缀但 `public` 的成员算不算公开面**：`Sha512` 的 7 个内部函数、`ParseResult` 的 `_` 字段、
   `public class _RepeatedList` / `_MutexGroup`。本批选择**不文档化**（视为应改 `internal` 的缺口），
   若短期不修，参考页与真实公开面不符。
3. **`z42.diagnostics` 是两件事捆一起**（日志 + VM 自省），本批按一页写完；若将来拆包，此页要跟着拆。
4. **`Std.Runtime.Clock` 无人认领**：在 `z42.core/src/Clock.z42`，命名空间 `Std.Runtime`（非 `Std.Time`）。
   本批放在 `time.md` 末尾并显式标注命名空间不同；将来若有 core/runtime 参考页应搬过去。
