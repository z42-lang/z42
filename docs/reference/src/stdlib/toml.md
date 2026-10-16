<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.toml/ -->

# z42.toml —— 把 TOML 1.0 文本解析成值树（[`TomlValue`]），或把值树写回 canonical TOML 文本。

> 包路径 `src/libraries/z42.toml/`；命名空间 `Std.Toml`

z42 自己的工程清单 `*.z42.toml` 与 `versions.toml` 就用它读。覆盖 TOML 1.0 的绝大部分语法，
唯一缺的大块是 4 种 datetime 类型——碰到日期字面量会报解析错误，不会静默当字符串吞掉。整份文档
一次读进内存，解析 fail-fast（抛第一个错误）。

## 不支持

- **datetime（TOML 1.0 的 4 种日期时间类型）**：`d = 1979-05-27T07:32:00Z` 与 `d = 1979-05-27`
  都在 `1979` 之后报 `expected newline or EOF after value`。需要日期就先按字符串存
  （`d = "1979-05-27"`），读出来自己解析。
- **注释保留 / 原格式保留**：解析丢掉注释与空行，`Stringify` 不还原。
- **输出格式可调**：只有一种 canonical 形态，没有缩进 / 数组换行 / pretty 选项。
- **Schema 校验**、默认值填充、类型强制。
- **删除键**：`TomlValue` 只有 `Set`，没有 remove；要去键就重建表。
- **多错误聚合**：解析在第一个错误处抛出，不返回错误列表。
- **`nan` 的 round-trip**：`nan` 能解析，但 `Stringify` 写出的是 `NaN.0`，再解析会失败。有 NaN 的
  值树别指望写回去还能读。（`inf` / `-inf` 的 round-trip 正常。）

## TomlException

TOML 解析 / 序列化错误，携带 1-based 行 / 列号，方便定位 manifest 中的语法错误。

```z42
public class TomlException : Exception {
    int Line
    int Column
    TomlException(string message)
    TomlException(string message, int line, int column)
}
```

### int Line

1-based 行号；0 = 位置不可用（如 stringify 阶段错误）。

### int Column

1-based 列号；0 = 位置不可用（如 stringify 阶段错误）。

### TomlException(string message)

不带位置的构造；`Line` / `Column` 置 0。

### TomlException(string message, int line, int column)

带 1-based 行 / 列号的构造。

## TomlParser


```z42
public sealed partial class TomlParser {
    TomlParser(string src)
    TomlValue ParseDocument()
}
```

### TomlParser(string src)

用源文本构造解析器。

### TomlValue ParseDocument()

解析整份文档，返回根 table。

## TomlParser

TOML 1.0 子集解析器（tokenizer + 递归下降）；日常用 `TomlValue.Parse` 即可，通常不直接构造。

```z42
public sealed partial class TomlParser {
    TomlParser(string src)
    TomlValue ParseDocument()
}
```

### TomlParser(string src)

用源文本构造解析器。

### TomlValue ParseDocument()

解析整份文档，返回根 table。

## TomlValue

值树的唯一节点类型，同时是包的入口点。

6 种 kind：String / Long / Double / Bool / Array / Table；用 `Is*()` / `As*()` 访问。

```z42
public class TomlValue {
    static TomlValue Parse(string text)
    static string Stringify(TomlValue root)
    static TomlValue ParseStream(Stream source)
    static void WriteTo(Stream dest, TomlValue root)
    static TomlValue OfString(string s)
    static TomlValue OfLong(long n)
    static TomlValue OfDouble(double d)
    static TomlValue OfBool(bool b)
    static TomlValue OfArray()
    static TomlValue OfTable()
    bool IsString()
    bool IsLong()
    bool IsDouble()
    bool IsBool()
    bool IsArray()
    bool IsTable()
    string KindName()
    string AsString()
    long AsLong()
    double AsDouble()
    bool AsBool()
    bool ContainsKey(string key)
    TomlValue Get(string key)
    bool TryGet(string key, ref TomlValue value)
    bool TryGetValue<T>(string key, ref T value)
    void Set(string key, TomlValue v)
    string[] Keys()
    int Count()
    int Length()
    TomlValue At(int i)
    void Add(TomlValue v)
}
```

### static TomlValue Parse(string text)

解析 TOML 文本为根 table。语法错误抛带行列的 `TomlException`；空输入得到空 table。

### static string Stringify(TomlValue root)

把值树序列化为 canonical TOML 文本。`root` 必须是 table，否则抛 `TomlException`。

键按**插入顺序**输出（解析出来的树即文件顺序），round-trip 不重排、diff 友好；注释不保留。

### static TomlValue ParseStream(Stream source)

从 UTF-8 字节流解析 TOML，读到 EOF，不关闭流。独立命名（非 `Parse` 重载）。

### static void WriteTo(Stream dest, TomlValue root)

把 canonical TOML 以 UTF-8 写入流，不关闭 `dest`。`root` 必须是 table。

### static TomlValue OfString(string s)

构造 String 节点。

### static TomlValue OfLong(long n)

构造 Long 节点。

### static TomlValue OfDouble(double d)

构造 Double 节点。

### static TomlValue OfBool(bool b)

构造 Bool 节点。

### static TomlValue OfArray()

构造空数组，用 `Add` 填充。

### static TomlValue OfTable()

构造空表，用 `Set` 填充。

### bool IsString()

kind 是否为 String。

### bool IsLong()

kind 是否为 Long。

### bool IsDouble()

kind 是否为 Double。

### bool IsBool()

kind 是否为 Bool。

### bool IsArray()

kind 是否为 Array。

### bool IsTable()

kind 是否为 Table。

### string KindName()

返回类型标签名（`"string"` / `"long"` / `"double"` / `"bool"` / `"array"` / `"table"`），用于诊断。

### string AsString()

取 String 值；kind 不符抛 `TomlException("expected string, got …")`。

### long AsLong()

取 Long 值；kind 不符抛 `TomlException`。

### double AsDouble()

取 Double 值；kind 不符抛 `TomlException`。不接受 Long（整数值要先自己转）。

### bool AsBool()

取 Bool 值；kind 不符抛 `TomlException`。

### bool ContainsKey(string key)

表是否含该键；非表值返回 false，不抛。

### TomlValue Get(string key)

取表值；键不存在抛 `TomlException("key not found: k")`。

### bool TryGet(string key, ref TomlValue value)

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。

本值不是 table 时返回 false 而**不抛** —— 与 `ContainsKey` 一致（它对非 table 也返回
false），于是 `if (t.ContainsKey(k)) { … t.Get(k) … }` 换成 `TryGet` 是逐字等价的重写。

### bool TryGetValue<T>(string key, ref T value)

取即检查 + 取标量：命中则按 `T` 取出值写入 `value` 并返回 true； 未命中返回 false 且**不动** `value` —— 所以「有就覆盖默认值」能一行写完： `t.TryGetValue<string>("name", ref name);`

⚠️ 命中但**类型不符照抛** `TomlException`，与 `AsString()` / `AsLong()` 一字不差。
「键不在」与「值类型错」是两种失败：前者是调用方要处理的常态，后者是 TOML 文件写错了。
把后者吞成 false 会让配置里的类型笔误**静默退回默认值** —— 那正是这套 API 要消灭的东西。

支持的 `T`：`string` / `long` / `double` / `bool` / `TomlValue`（原样取子树，
等价于 `TryGet`，供泛型调用点写法统一）。其它 `T` 抛异常 —— 那是**调用方写错了代码**，
不是数据的问题，返回 false 会把它伪装成「键不在」。

### void Set(string key, TomlValue v)

写入键值；覆盖已有键时保持原位置（round-trip 不重排）。

### string[] Keys()

当前表键的插入顺序快照。

### int Count()

表键数（数组则为元素数）；其余 kind 抛 `TomlException`。

### int Length()

数组元素数；非数组抛 `TomlException`。

### TomlValue At(int i)

取数组元素；越界抛 `TomlException`（不是 VM abort）。

### void Add(TomlValue v)

追加元素到数组末尾。

## TomlWriter

值树 → TOML 文本的序列化器；日常用 `TomlValue.Stringify` 即可，通常不直接构造。

```z42
public sealed class TomlWriter {
    TomlWriter()
    string Write(TomlValue root)
}
```

### TomlWriter()

构造一个空 writer。

### string Write(TomlValue root)

把值树序列化为 TOML 文本。

