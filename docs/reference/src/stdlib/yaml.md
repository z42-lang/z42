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

[`src/libraries/z42.yaml/src/YamlException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlException.z42#L7)

`Std.Yaml.*` 抛出的用户可见异常：解析错误 / 访问器类型不符 / 序列化错误。

与其余 stdlib 异常一样住在所属包的命名空间（`Std.Toml.TomlException` /
`Std.Json.JsonException`）。

```z42
public class YamlException : Exception {
    YamlException(string message)
}
```

成员：[`YamlException`](#Std.Yaml.YamlException.ctor(string))

<a id="Std.Yaml.YamlException.ctor(string)"></a>
### YamlException(string message)

[`src/libraries/z42.yaml/src/YamlException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlException.z42#L9)

构造带消息的 YamlException。

## YamlParser

[`src/libraries/z42.yaml/src/YamlParser.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlParser.z42#L33)

YAML 1.2 子集解析器；`YamlValue.Parse*` 的底层载体，日常代码用 `YamlValue` 即可。

```z42
public sealed class YamlParser {
    YamlParser(string src)
    YamlValue ParseDocument()
    YamlValue[] ParseAllDocuments()
}
```

成员：[`YamlParser`](#Std.Yaml.YamlParser.ctor(string)) · [`ParseDocument`](#Std.Yaml.YamlParser.ParseDocument()) · [`ParseAllDocuments`](#Std.Yaml.YamlParser.ParseAllDocuments())

<a id="Std.Yaml.YamlParser.ctor(string)"></a>
### YamlParser(string src)

[`src/libraries/z42.yaml/src/YamlParser.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlParser.z42#L69)

构造解析器（绑定源文本）。

<a id="Std.Yaml.YamlParser.ParseDocument()"></a>
### YamlValue ParseDocument()

[`src/libraries/z42.yaml/src/YamlParser.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlParser.z42#L87)

解析单个文档；文档后有多余尾随内容（第二个文档）抛 YamlException。

<a id="Std.Yaml.YamlParser.ParseAllDocuments()"></a>
### YamlValue[] ParseAllDocuments()

[`src/libraries/z42.yaml/src/YamlParser.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlParser.z42#L103)

多文档入口：按序返回每个 `---` 分隔的文档。空 / 全空白输入返回空数组。

## YamlValue

[`src/libraries/z42.yaml/src/YamlValue.z42:58`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L58)

值树的唯一节点类型，也是包的入口点。

8 种 kind：Null / Bool / Int / Float / String / Sequence / Mapping / Timestamp。

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

成员：[`Parse`](#Std.Yaml.YamlValue.Parse(string)) · [`ParseAll`](#Std.Yaml.YamlValue.ParseAll(string)) · [`Stringify`](#Std.Yaml.YamlValue.Stringify(YamlValue)) · [`ParseStream`](#Std.Yaml.YamlValue.ParseStream(Stream)) · [`ParseAllStream`](#Std.Yaml.YamlValue.ParseAllStream(Stream)) · [`WriteTo`](#Std.Yaml.YamlValue.WriteTo(Stream,YamlValue)) · [`OfNull`](#Std.Yaml.YamlValue.OfNull()) · [`OfBool`](#Std.Yaml.YamlValue.OfBool(bool)) · [`OfInt`](#Std.Yaml.YamlValue.OfInt(long)) · [`OfFloat`](#Std.Yaml.YamlValue.OfFloat(double)) · [`OfString`](#Std.Yaml.YamlValue.OfString(string)) · [`OfTimestamp`](#Std.Yaml.YamlValue.OfTimestamp(DateTime)) · [`OfTimestampString`](#Std.Yaml.YamlValue.OfTimestampString(string)) · [`OfSequence`](#Std.Yaml.YamlValue.OfSequence()) · [`OfMapping`](#Std.Yaml.YamlValue.OfMapping()) · [`IsNull`](#Std.Yaml.YamlValue.IsNull()) · [`IsBool`](#Std.Yaml.YamlValue.IsBool()) · [`IsInt`](#Std.Yaml.YamlValue.IsInt()) · [`IsFloat`](#Std.Yaml.YamlValue.IsFloat()) · [`IsString`](#Std.Yaml.YamlValue.IsString()) · [`IsSequence`](#Std.Yaml.YamlValue.IsSequence()) · [`IsMapping`](#Std.Yaml.YamlValue.IsMapping()) · [`IsTimestamp`](#Std.Yaml.YamlValue.IsTimestamp()) · [`KindName`](#Std.Yaml.YamlValue.KindName()) · [`AsBool`](#Std.Yaml.YamlValue.AsBool()) · [`AsInt`](#Std.Yaml.YamlValue.AsInt()) · [`AsFloat`](#Std.Yaml.YamlValue.AsFloat()) · [`AsString`](#Std.Yaml.YamlValue.AsString()) · [`AsTimestamp`](#Std.Yaml.YamlValue.AsTimestamp()) · [`AsTimestampRaw`](#Std.Yaml.YamlValue.AsTimestampRaw()) · [`Length`](#Std.Yaml.YamlValue.Length()) · [`At`](#Std.Yaml.YamlValue.At(int)) · [`Add`](#Std.Yaml.YamlValue.Add(YamlValue)) · [`ContainsKey`](#Std.Yaml.YamlValue.ContainsKey(string)) · [`Get`](#Std.Yaml.YamlValue.Get(string)) · [`TryGet`](#Std.Yaml.YamlValue.TryGet(string,YamlValue)) · [`TryGetValue`](#Std.Yaml.YamlValue.TryGetValue%3CT%3E(string,T)) · [`Set`](#Std.Yaml.YamlValue.Set(string,YamlValue)) · [`Keys`](#Std.Yaml.YamlValue.Keys()) · [`Count`](#Std.Yaml.YamlValue.Count()) · [`DeepClone`](#Std.Yaml.YamlValue.DeepClone())

<a id="Std.Yaml.YamlValue.Parse(string)"></a>
### static YamlValue Parse(string text)

[`src/libraries/z42.yaml/src/YamlValue.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L83)

解析 YAML 文本为根值。语法错误抛 YamlException（消息含 行:列）。

空输入返回 Null 值；多文档输入（`---` 分隔）报错，请用 `ParseAll`。

<a id="Std.Yaml.YamlValue.ParseAll(string)"></a>
### static YamlValue[] ParseAll(string text)

[`src/libraries/z42.yaml/src/YamlValue.z42:89`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L89)

解析 `---` 分隔的多文档 YAML 为值数组（kubectl / helm 清单风格）。空 / 全空白输入返回空数组。

<a id="Std.Yaml.YamlValue.Stringify(YamlValue)"></a>
### static string Stringify(YamlValue root)

[`src/libraries/z42.yaml/src/YamlValue.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L95)

把值树序列化为块风格 YAML 文本。输出便于人读，但并非与输入逐字节一致（不保留注释 / 引号风格）。

<a id="Std.Yaml.YamlValue.ParseStream(Stream)"></a>
### static YamlValue ParseStream(Stream source)

[`src/libraries/z42.yaml/src/YamlValue.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L106)

从 UTF-8 字节流解析 YAML。独立命名（非 `Parse` 的重载），不关闭传入的流。

<a id="Std.Yaml.YamlValue.ParseAllStream(Stream)"></a>
### static YamlValue[] ParseAllStream(Stream source)

[`src/libraries/z42.yaml/src/YamlValue.z42:114`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L114)

从 UTF-8 字节流解析多文档 YAML 为值数组。不关闭传入的流。

<a id="Std.Yaml.YamlValue.WriteTo(Stream,YamlValue)"></a>
### static void WriteTo(Stream dest, YamlValue root)

[`src/libraries/z42.yaml/src/YamlValue.z42:122`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L122)

把值树序列化为 UTF-8 字节写入 `dest`。不关闭 `dest`。

<a id="Std.Yaml.YamlValue.OfNull()"></a>
### static YamlValue OfNull()

[`src/libraries/z42.yaml/src/YamlValue.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L131)

构造 Null 值。

<a id="Std.Yaml.YamlValue.OfBool(bool)"></a>
### static YamlValue OfBool(bool b)

[`src/libraries/z42.yaml/src/YamlValue.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L138)

构造 Bool 值。

<a id="Std.Yaml.YamlValue.OfInt(long)"></a>
### static YamlValue OfInt(long n)

[`src/libraries/z42.yaml/src/YamlValue.z42:146`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L146)

构造 Int 值。

<a id="Std.Yaml.YamlValue.OfFloat(double)"></a>
### static YamlValue OfFloat(double d)

[`src/libraries/z42.yaml/src/YamlValue.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L154)

构造 Float 值。

<a id="Std.Yaml.YamlValue.OfString(string)"></a>
### static YamlValue OfString(string s)

[`src/libraries/z42.yaml/src/YamlValue.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L162)

构造 String 值。

<a id="Std.Yaml.YamlValue.OfTimestamp(DateTime)"></a>
### static YamlValue OfTimestamp(DateTime dt)

[`src/libraries/z42.yaml/src/YamlValue.z42:172`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L172)

从 DateTime 构造时间戳；`Stringify` 输出规范 UTC ISO 8601。

如需保留非 UTC 偏移 / 不同精度，用 `OfTimestampString(raw)`。

<a id="Std.Yaml.YamlValue.OfTimestampString(string)"></a>
### static YamlValue OfTimestampString(string iso)

[`src/libraries/z42.yaml/src/YamlValue.z42:181`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L181)

从 ISO 8601 串构造时间戳，保留原词法串以便往返保真。非法输入抛 ArgumentException。

<a id="Std.Yaml.YamlValue.OfSequence()"></a>
### static YamlValue OfSequence()

[`src/libraries/z42.yaml/src/YamlValue.z42:190`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L190)

构造空序列（用 `Add` 填充）。

<a id="Std.Yaml.YamlValue.OfMapping()"></a>
### static YamlValue OfMapping()

[`src/libraries/z42.yaml/src/YamlValue.z42:199`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L199)

构造空映射（用 `Set` 填充）。

<a id="Std.Yaml.YamlValue.IsNull()"></a>
### bool IsNull()

[`src/libraries/z42.yaml/src/YamlValue.z42:211`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L211)

是否为 Null。

<a id="Std.Yaml.YamlValue.IsBool()"></a>
### bool IsBool()

[`src/libraries/z42.yaml/src/YamlValue.z42:213`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L213)

是否为 Bool。

<a id="Std.Yaml.YamlValue.IsInt()"></a>
### bool IsInt()

[`src/libraries/z42.yaml/src/YamlValue.z42:215`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L215)

是否为 Int。

<a id="Std.Yaml.YamlValue.IsFloat()"></a>
### bool IsFloat()

[`src/libraries/z42.yaml/src/YamlValue.z42:217`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L217)

是否为 Float。

<a id="Std.Yaml.YamlValue.IsString()"></a>
### bool IsString()

[`src/libraries/z42.yaml/src/YamlValue.z42:219`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L219)

是否为 String。

<a id="Std.Yaml.YamlValue.IsSequence()"></a>
### bool IsSequence()

[`src/libraries/z42.yaml/src/YamlValue.z42:221`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L221)

是否为 Sequence。

<a id="Std.Yaml.YamlValue.IsMapping()"></a>
### bool IsMapping()

[`src/libraries/z42.yaml/src/YamlValue.z42:223`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L223)

是否为 Mapping。

<a id="Std.Yaml.YamlValue.IsTimestamp()"></a>
### bool IsTimestamp()

[`src/libraries/z42.yaml/src/YamlValue.z42:225`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L225)

是否为 Timestamp。

<a id="Std.Yaml.YamlValue.KindName()"></a>
### string KindName()

[`src/libraries/z42.yaml/src/YamlValue.z42:228`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L228)

返回 kind 的类型名（用于诊断）。

<a id="Std.Yaml.YamlValue.AsBool()"></a>
### bool AsBool()

[`src/libraries/z42.yaml/src/YamlValue.z42:244`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L244)

取 Bool；kind 不符抛 YamlException。

<a id="Std.Yaml.YamlValue.AsInt()"></a>
### long AsInt()

[`src/libraries/z42.yaml/src/YamlValue.z42:252`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L252)

取 Int（long）；不接受 Float。

<a id="Std.Yaml.YamlValue.AsFloat()"></a>
### double AsFloat()

[`src/libraries/z42.yaml/src/YamlValue.z42:260`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L260)

取 Float（double）；不接受 Int。

<a id="Std.Yaml.YamlValue.AsString()"></a>
### string AsString()

[`src/libraries/z42.yaml/src/YamlValue.z42:268`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L268)

取 String。

<a id="Std.Yaml.YamlValue.AsTimestamp()"></a>
### DateTime AsTimestamp()

[`src/libraries/z42.yaml/src/YamlValue.z42:276`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L276)

取 Timestamp 的 DateTime。

<a id="Std.Yaml.YamlValue.AsTimestampRaw()"></a>
### string AsTimestampRaw()

[`src/libraries/z42.yaml/src/YamlValue.z42:284`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L284)

取解析得来的原词法串（由 `OfTimestamp` 构造时为空串），便于往返保真。

<a id="Std.Yaml.YamlValue.Length()"></a>
### int Length()

[`src/libraries/z42.yaml/src/YamlValue.z42:294`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L294)

序列元素数；非序列抛 YamlException。

<a id="Std.Yaml.YamlValue.At(int)"></a>
### YamlValue At(int i)

[`src/libraries/z42.yaml/src/YamlValue.z42:302`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L302)

取第 i 个元素；越界抛 YamlException（不是 VM abort）。

<a id="Std.Yaml.YamlValue.Add(YamlValue)"></a>
### void Add(YamlValue v)

[`src/libraries/z42.yaml/src/YamlValue.z42:313`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L313)

追加元素到序列末尾。

<a id="Std.Yaml.YamlValue.ContainsKey(string)"></a>
### bool ContainsKey(string key)

[`src/libraries/z42.yaml/src/YamlValue.z42:332`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L332)

是否含键；非映射值返回 false，不抛。

<a id="Std.Yaml.YamlValue.Get(string)"></a>
### YamlValue Get(string key)

[`src/libraries/z42.yaml/src/YamlValue.z42:343`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L343)

取键对应值；键不存在抛 YamlException("key not found: k")。

<a id="Std.Yaml.YamlValue.TryGet(string,YamlValue)"></a>
### bool TryGet(string key, ref YamlValue value)

[`src/libraries/z42.yaml/src/YamlValue.z42:369`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L369)

取即检查：命中则写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。

本值不是 mapping 时返回 false 而**不抛** —— 与 `ContainsKey` 一致。

<a id="Std.Yaml.YamlValue.TryGetValue<T>(string,T)"></a>
### bool TryGetValue<T>(string key, ref T value)

[`src/libraries/z42.yaml/src/YamlValue.z42:391`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L391)

取即检查 + 取标量：命中则按 `T` 取值写入 `value` 并返回 true；未命中返回 false 且**不动** `value`。

「有就覆盖默认值」一行写完。

⚠️ 命中但**类型不符照抛** `YamlException`，与 `AsString()` / `AsInt()` 一字不差。

支持的 `T`：`string` / `long`（`AsInt`）/ `double`（`AsFloat`）/ `bool` /
`DateTime`（`AsTimestamp`）/ `YamlValue`（原样取子树）。其它 `T` 抛异常。

⚠️ 这里 `long` 走 `AsInt`、`double` 走 `AsFloat` —— YAML 的访问器名与 TOML / JSON
**不同名但同语义**（`AsInt` 返回的是 i64）。按 **z42 类型**选重载，别按访问器名字猜。

<a id="Std.Yaml.YamlValue.Set(string,YamlValue)"></a>
### void Set(string key, YamlValue v)

[`src/libraries/z42.yaml/src/YamlValue.z42:406`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L406)

写键值对（键已存在则覆盖）。

<a id="Std.Yaml.YamlValue.Keys()"></a>
### string[] Keys()

[`src/libraries/z42.yaml/src/YamlValue.z42:438`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L438)

当前映射键的插入顺序快照。

<a id="Std.Yaml.YamlValue.Count()"></a>
### int Count()

[`src/libraries/z42.yaml/src/YamlValue.z42:452`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L452)

序列元素数 / 映射键数；其余 kind 抛 YamlException。

<a id="Std.Yaml.YamlValue.DeepClone()"></a>
### YamlValue DeepClone()

[`src/libraries/z42.yaml/src/YamlValue.z42:459`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlValue.z42#L459)

递归复制整棵子树。别名（`*name`）解析出的值就是锚点值的 DeepClone。

## YamlWriter

[`src/libraries/z42.yaml/src/YamlWriter.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlWriter.z42#L28)

YAML 块风格序列化器；`YamlValue.Stringify` / `WriteTo` 的底层载体，日常代码用 `YamlValue` 即可。

```z42
public sealed class YamlWriter {
    string Write(YamlValue root)
}
```

成员：[`Write`](#Std.Yaml.YamlWriter.Write(YamlValue))

<a id="Std.Yaml.YamlWriter.Write(YamlValue)"></a>
### string Write(YamlValue root)

[`src/libraries/z42.yaml/src/YamlWriter.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.yaml/src/YamlWriter.z42#L30)

把值树序列化为块风格 YAML 文本。

