<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.yaml/ -->

# z42.yaml —— YAML 读写。

> 包路径 `src/libraries/z42.yaml/`；命名空间 `Std.Yaml`

把 [YAML 1.2](https://yaml.org/spec/1.2.2/) 文本解析成值树（`YamlValue`），或把值树写成块风格
YAML。覆盖 Docker Compose / kubectl / Helm 这类配置日常用到的子集：块映射与块序列、flow 风格、
块标量、锚点与别名、merge key、类型 tag、多文档流、时间戳。

两条最容易踩的边界：

1. `- ` 序列项下面跟多行的嵌套映射不支持（`- name: bob` 换行后再写 `age: 3` 会报错）；
   写成 `-` 单独一行 + 下一层缩进的映射即可。
2. 复杂键（`? key` 语法）与 `%YAML` 指令不支持。

整份文档一次读进内存，解析 fail-fast（抛第一个错误）。

## 不支持

- 序列项下的多行嵌套映射——最常踩的一条：每个 `- ` 只支持同一行内的一对 `k: v`；多字段写法是让
  `-` 单独占一行。同理 `- - 1` 这种同行嵌套序列也不支持，得换成 `-` 单独一行 + 下一层的块序列。
- 复杂键（`? key` 后跟 `: value` 的语法）：映射键只能是字符串。
- `%YAML` / `%TAG` 指令：指令行会被当成文档内容，导致解析报错。
- flow 上下文里的锚点声明：`[*alias]` 可以用，`[&a 1, *a]` 里的 `&a 1` 会被当成普通标量，
  随后引用它的别名报 `undefined anchor`。
- Stringify 的 flow 风格 / 注释保留 / 锚点回写：输出恒为块风格的展开形态。
- Schema 校验、对象 ↔ YAML 的 serde（只有 `z42.json` 提供 serde 层）。
- 增量 / 流式解析：`ParseStream` 只是「读到 EOF 再整体解析」。
- 多错误聚合：解析在第一个错误处抛出，不返回错误列表。

## YamlException

`src/libraries/z42.yaml/src/YamlException.z42:5`

`Std.Yaml.*` 抛出的用户可见异常：解析错误 / 访问器类型不符 / 序列化错误。 与其余 stdlib 异常一样住在所属包的命名空间（`Std.Toml.TomlException` / `Std.Json.JsonException`）。

```z42
public class YamlException : Exception {
    YamlException(string message)
}
```

### YamlException(string message)

`src/libraries/z42.yaml/src/YamlException.z42:7`

构造带消息的 YamlException。

## YamlParser

`src/libraries/z42.yaml/src/YamlParser.z42:33`

YAML 1.2 子集解析器；`YamlValue.Parse*` 的底层载体，日常代码用 `YamlValue` 即可。

```z42
public sealed class YamlParser {
    YamlParser(string src)
    YamlValue ParseDocument()
    YamlValue[] ParseAllDocuments()
}
```

### YamlParser(string src)

`src/libraries/z42.yaml/src/YamlParser.z42:69`

构造解析器（绑定源文本）。

### YamlValue ParseDocument()

`src/libraries/z42.yaml/src/YamlParser.z42:87`

解析单个文档；文档后有多余尾随内容（第二个文档）抛 YamlException。

### YamlValue[] ParseAllDocuments()

`src/libraries/z42.yaml/src/YamlParser.z42:103`

多文档入口：按序返回每个 `---` 分隔的文档。空 / 全空白输入返回空数组。

## YamlValue

`src/libraries/z42.yaml/src/YamlValue.z42:56`

值树的唯一节点类型，也是包的入口点；8 种 kind：Null / Bool / Int / Float / String / Sequence / Mapping / Timestamp。

```z42
public class YamlValue {
    static YamlValue Parse(string text)
    static YamlValue[] ParseAll(string text)
    static string Stringify(YamlValue root)
    static YamlValue ParseStream(Stream source)
    static YamlValue[] ParseAllStream(Stream source)
    static void WriteTo(Stream dest, YamlValue root)
    static YamlValue OfNull()
    static YamlValue OfBool(bool b)
    static YamlValue OfInt(long n)
    static YamlValue OfFloat(double d)
    static YamlValue OfString(string s)
    static YamlValue OfTimestamp(DateTime dt)
    static YamlValue OfTimestampString(string iso)
    static YamlValue OfSequence()
    static YamlValue OfMapping()
    bool IsNull()
    bool IsBool()
    bool IsInt()
    bool IsFloat()
    bool IsString()
    bool IsSequence()
    bool IsMapping()
    bool IsTimestamp()
    string KindName()
    bool AsBool()
    long AsInt()
    double AsFloat()
    string AsString()
    DateTime AsTimestamp()
    string AsTimestampRaw()
    int Length()
    YamlValue At(int i)
    void Add(YamlValue v)
    bool ContainsKey(string key)
    YamlValue Get(string key)
    bool TryGet(string key, ref YamlValue value)
    bool TryGetValue<T>(string key, ref T value)
    void Set(string key, YamlValue v)
    string[] Keys()
    int Count()
    YamlValue DeepClone()
}
```

### static YamlValue Parse(string text)

`src/libraries/z42.yaml/src/YamlValue.z42:80`

解析 YAML 文本为根值。语法错误抛 YamlException（消息含 行:列）；空输入返回 Null 值； 多文档输入（`---` 分隔）报错，请用 `ParseAll`。

### static YamlValue[] ParseAll(string text)

`src/libraries/z42.yaml/src/YamlValue.z42:86`

解析 `---` 分隔的多文档 YAML 为值数组（kubectl / helm 清单风格）。空 / 全空白输入返回空数组。

### static string Stringify(YamlValue root)

`src/libraries/z42.yaml/src/YamlValue.z42:92`

把值树序列化为块风格 YAML 文本。输出便于人读，但并非与输入逐字节一致（不保留注释 / 引号风格）。

### static YamlValue ParseStream(Stream source)

`src/libraries/z42.yaml/src/YamlValue.z42:103`

从 UTF-8 字节流解析 YAML。独立命名（非 `Parse` 的重载），不关闭传入的流。

### static YamlValue[] ParseAllStream(Stream source)

`src/libraries/z42.yaml/src/YamlValue.z42:111`

从 UTF-8 字节流解析多文档 YAML 为值数组。不关闭传入的流。

### static void WriteTo(Stream dest, YamlValue root)

`src/libraries/z42.yaml/src/YamlValue.z42:119`

把值树序列化为 UTF-8 字节写入 `dest`。不关闭 `dest`。

### static YamlValue OfNull()

`src/libraries/z42.yaml/src/YamlValue.z42:128`

构造 Null 值。

### static YamlValue OfBool(bool b)

`src/libraries/z42.yaml/src/YamlValue.z42:135`

构造 Bool 值。

### static YamlValue OfInt(long n)

`src/libraries/z42.yaml/src/YamlValue.z42:143`

构造 Int 值。

### static YamlValue OfFloat(double d)

`src/libraries/z42.yaml/src/YamlValue.z42:151`

构造 Float 值。

### static YamlValue OfString(string s)

`src/libraries/z42.yaml/src/YamlValue.z42:159`

构造 String 值。

### static YamlValue OfTimestamp(DateTime dt)

`src/libraries/z42.yaml/src/YamlValue.z42:168`

从 DateTime 构造时间戳；`Stringify` 输出规范 UTC ISO 8601。 如需保留非 UTC 偏移 / 不同精度，用 `OfTimestampString(raw)`。

### static YamlValue OfTimestampString(string iso)

`src/libraries/z42.yaml/src/YamlValue.z42:177`

从 ISO 8601 串构造时间戳，保留原词法串以便往返保真。非法输入抛 ArgumentException。

### static YamlValue OfSequence()

`src/libraries/z42.yaml/src/YamlValue.z42:186`

构造空序列（用 `Add` 填充）。

### static YamlValue OfMapping()

`src/libraries/z42.yaml/src/YamlValue.z42:195`

构造空映射（用 `Set` 填充）。

### bool IsNull()

`src/libraries/z42.yaml/src/YamlValue.z42:207`

是否为 Null。

### bool IsBool()

`src/libraries/z42.yaml/src/YamlValue.z42:209`

是否为 Bool。

### bool IsInt()

`src/libraries/z42.yaml/src/YamlValue.z42:211`

是否为 Int。

### bool IsFloat()

`src/libraries/z42.yaml/src/YamlValue.z42:213`

是否为 Float。

### bool IsString()

`src/libraries/z42.yaml/src/YamlValue.z42:215`

是否为 String。

### bool IsSequence()

`src/libraries/z42.yaml/src/YamlValue.z42:217`

是否为 Sequence。

### bool IsMapping()

`src/libraries/z42.yaml/src/YamlValue.z42:219`

是否为 Mapping。

### bool IsTimestamp()

`src/libraries/z42.yaml/src/YamlValue.z42:221`

是否为 Timestamp。

### string KindName()

`src/libraries/z42.yaml/src/YamlValue.z42:224`

返回 kind 的类型名（用于诊断）。

### bool AsBool()

`src/libraries/z42.yaml/src/YamlValue.z42:240`

取 Bool；kind 不符抛 YamlException。

### long AsInt()

`src/libraries/z42.yaml/src/YamlValue.z42:248`

取 Int（long）；不接受 Float。

### double AsFloat()

`src/libraries/z42.yaml/src/YamlValue.z42:256`

取 Float（double）；不接受 Int。

### string AsString()

`src/libraries/z42.yaml/src/YamlValue.z42:264`

取 String。

### DateTime AsTimestamp()

`src/libraries/z42.yaml/src/YamlValue.z42:272`

取 Timestamp 的 DateTime。

### string AsTimestampRaw()

`src/libraries/z42.yaml/src/YamlValue.z42:280`

取解析得来的原词法串（由 `OfTimestamp` 构造时为空串），便于往返保真。

### int Length()

`src/libraries/z42.yaml/src/YamlValue.z42:290`

序列元素数；非序列抛 YamlException。

### YamlValue At(int i)

`src/libraries/z42.yaml/src/YamlValue.z42:298`

取第 i 个元素；越界抛 YamlException（不是 VM abort）。

### void Add(YamlValue v)

`src/libraries/z42.yaml/src/YamlValue.z42:309`

追加元素到序列末尾。

### bool ContainsKey(string key)

`src/libraries/z42.yaml/src/YamlValue.z42:328`

是否含键；非映射值返回 false，不抛。

### YamlValue Get(string key)

`src/libraries/z42.yaml/src/YamlValue.z42:339`

取键对应值；键不存在抛 YamlException("key not found: k")。

### bool TryGet(string key, ref YamlValue value)

`src/libraries/z42.yaml/src/YamlValue.z42:364`

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。 本值不是 mapping 时返回 false 而**不抛** —— 与 `ContainsKey` 一致。

### bool TryGetValue<T>(string key, ref T value)

`src/libraries/z42.yaml/src/YamlValue.z42:384`

取即检查 + 取标量：命中则按 `T` 取值写入 `value` 并返回 true；未命中返回 false 且 **不动** `value`（「有就覆盖默认值」一行写完）。

⚠️ 命中但**类型不符照抛** `YamlException`，与 `AsString()` / `AsInt()` 一字不差。

支持的 `T`：`string` / `long`（`AsInt`）/ `double`（`AsFloat`）/ `bool` /
`DateTime`（`AsTimestamp`）/ `YamlValue`（原样取子树）。其它 `T` 抛异常。

⚠️ 这里 `long` 走 `AsInt`、`double` 走 `AsFloat` —— YAML 的访问器名与 TOML / JSON
**不同名但同语义**（`AsInt` 返回的是 i64）。按 **z42 类型**选重载，别按访问器名字猜。

### void Set(string key, YamlValue v)

`src/libraries/z42.yaml/src/YamlValue.z42:399`

写键值对（键已存在则覆盖）。

### string[] Keys()

`src/libraries/z42.yaml/src/YamlValue.z42:431`

当前映射键的插入顺序快照。

### int Count()

`src/libraries/z42.yaml/src/YamlValue.z42:445`

序列元素数 / 映射键数；其余 kind 抛 YamlException。

### YamlValue DeepClone()

`src/libraries/z42.yaml/src/YamlValue.z42:452`

递归复制整棵子树。别名（`*name`）解析出的值就是锚点值的 DeepClone。

## YamlWriter

`src/libraries/z42.yaml/src/YamlWriter.z42:28`

YAML 块风格序列化器；`YamlValue.Stringify` / `WriteTo` 的底层载体，日常代码用 `YamlValue` 即可。

```z42
public sealed class YamlWriter {
    string Write(YamlValue root)
}
```

### string Write(YamlValue root)

`src/libraries/z42.yaml/src/YamlWriter.z42:30`

把值树序列化为块风格 YAML 文本。

