# z42.json —— JSON 读写与对象序列化

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.json/`；命名空间 `Std.Json`（异常类 `JsonException` 在 `Std`）

两层 API，按需取用：

- **DOM 层**（`JsonValue`）—— 把 JSON 文本解析成值树，逐键 / 逐下标访问；也可手工搭树再输出。
  `Parse` 严格遵循 [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)，`ParseRelaxed` 额外接受
  JSON5 风格的注释与尾逗号（给 `tsconfig.json` 这类工具配置用）。
- **serde 层**（`JsonSerializer`）—— 经反射在用户对象与 JSON 文本之间直转，不用写 DOM 代码。

整份文档一次性读进内存：没有增量 / 流式 reader，超大 JSON 需自行分块。解析失败一律
**fail-fast**，抛第一个错误，不聚合错误列表。

## JsonValue

值树的唯一节点类型，同时是整个包的入口点。一个 `JsonValue` 属于 7 种 kind 之一：

| kind | `KindName()` | 载荷 |
|---|---|---|
| Null | `"null"` | —— |
| Bool | `"bool"` | `bool` |
| Long | `"long"` | `long`（i64） |
| Double | `"double"` | `double`（f64） |
| String | `"string"` | `string` |
| Array | `"array"` | 有序 `JsonValue` 列表 |
| Object | `"object"` | 有序 key → `JsonValue`（键为字符串） |

### 入口点

```z42
// 解析（Stream 重载不关闭传入的 Stream，生命周期归调用方）
public static JsonValue Parse(string text)                  // 严格 RFC 8259
public static JsonValue ParseRelaxed(string text)           // + 注释 / 尾逗号 / NaN / Infinity
public static JsonValue ParseStream(Std.IO.Stream source)   // UTF-8，读到 EOF
public static JsonValue ParseRelaxedStream(Std.IO.Stream source)

// 序列化
public static string Stringify(JsonValue v)                 // 紧凑，无空白
public static string StringifyPretty(JsonValue v)           // 2 空格缩进
public static void WriteTo(Std.IO.Stream dest, JsonValue v)
public static void WriteToPretty(Std.IO.Stream dest, JsonValue v)
```

Stream 入口是**独立的名字**（`ParseStream` / `ParseRelaxedStream`），不是 `Parse` 的重载——
别去找 `Parse(Stream)`，没有。[TOML](toml.md) / [YAML](yaml.md) 两个包同样命名。

### 构造

```z42
public static JsonValue OfNull()
public static JsonValue OfBool(bool b)
public static JsonValue OfLong(long n)
public static JsonValue OfDouble(double d)
public static JsonValue OfString(string s)
public static JsonValue OfArray()        // 空数组，用 Add 填
public static JsonValue OfObject()       // 空对象，用 Set 填
```

### 判别与取值

```z42
public bool IsNull()   public bool IsBool()   public bool IsLong()   public bool IsDouble()
public bool IsString() public bool IsArray()  public bool IsObject()
public string KindName()

public bool   AsBool()     // kind 不符 → JsonException
public long   AsLong()
public double AsDouble()   // Long 也接受，自动提升为 double
public string AsString()
```

`AsDouble()` 是唯一放宽的取值器：kind 为 Long 时返回 `(double)` 提升值。其余 `As*` 严格
匹配 kind，不符即抛 `JsonException("expected xxx, got yyy")`。

### 对象操作（kind == Object）

```z42
public bool      ContainsKey(string key)   // 非对象值返回 false，不抛
public JsonValue Get(string key)           // 键不存在 → JsonException
public void      Set(string key, JsonValue v)
public string[]  Keys()                    // 插入顺序快照
public int       Count()                   // 对象键数；数组则为元素数；其余 kind 抛
```

- **键顺序稳定**：`Keys()`、`Stringify` 与解析顺序一致；`Set` 覆盖已有键时**保持原位置**。
- **重复键 last-wins**：`{"k":1,"k":2}` 解析成单键 `k = 2`，位置留在首次出现处
  （RFC 8259 未规定此情形）。

### 数组操作（kind == Array）

```z42
public int       Length()        // 非数组 → JsonException
public JsonValue At(int i)       // 越界 → JsonException（不是 VM abort）
public void      Add(JsonValue v)
public int       Count()         // 与 Length() 同值
```

## 数字与文本的具体行为

| 场景 | 行为 |
|---|---|
| 整数字面量 | 解析为 **Long**（i64） |
| 整数溢出 i64 | 回落为 **Double**，精度有损，不报错 |
| 带小数点 / 指数 | 解析为 **Double** |
| `42.0` 经 `Stringify` | 输出 `42` —— 再解析回来 kind 变成 Long |
| `\uXXXX` 转义 | 支持；代理对 `😀` 合成单个增补平面码点 |
| 非 ASCII 字符 | `Stringify` 原样输出 UTF-8，不转义 |
| 输出转义集 | `\" \\ \n \t \r \b \f` + `\u00XX`（其余控制字符） |
| 嵌套层数 | 上限 **256**，超出抛 `JsonException("nesting too deep (max 256)")` |

## 严格模式 vs relaxed 模式

`Parse` / `ParseStream` 覆盖完整 RFC 8259，不多不少。`ParseRelaxed` / `ParseRelaxedStream`
在此基础上额外接受：

| relaxed 额外接受 | 例子 |
|---|---|
| `//` 行注释 | `{ // 说明`<br>`  "a": 1 }` |
| `/* */` 块注释（可跨行；未闭合 → `JsonException`） | `/* 说明 */ {"a":1}` |
| 数组 / 对象尾逗号 | `[1, 2, ]` / `{"a": 1, }` |
| 特殊浮点 `NaN` / `Infinity` / `-Infinity` / `+Infinity` / `-NaN` | `{"x": Infinity}` |

严格模式对以上输入一律报错。relaxed 模式**不**接受：无引号键、单引号字符串、十六进制 /
前导点 / 尾随点数字字面量、`\xNN` 短转义。

## JsonPath —— 单值路径选择

```z42
public static JsonValue JsonPath.Select(JsonValue root, string path)
```

支持的路径语法：

| 语法 | 含义 |
|---|---|
| `$` | 根锚点（可省略） |
| `.name` | 对象键 |
| `["name"]` / `['name']` | 对象键（键里含 `.`、括号、Unicode 时用它） |
| `[n]` | 数组下标（非负十进制） |
| 段间空格 / 制表符 | 容忍 |

**返回语义**：路径任一步走不通（键缺失、下标越界、kind 不符）→ 返回 `null`；
只有**路径语法本身**非法才抛 `JsonException`。

不支持：`..` 递归下降、`*` 通配（显式抛异常，不静默返回 null）、`?(...)` 过滤、
`[start:end]` 切片、函数调用。

## JsonException

```z42
namespace Std;
public class JsonException : Exception {
    public int Line;      // 1-based；0 = 位置不可用（如 stringify 阶段错误）
    public int Column;
    public JsonException(string message)
    public JsonException(string message, int line, int column)
    override string ToString()   // "JsonException at 3:17: <message>"
}
```

解析错误、kind 不符的 `As*` / `Get` / `Length` 调用、JsonPath 语法错误都用它。

## JsonSerializer —— 对象 ↔ JSON

```z42
namespace Std.Json;
public static class JsonSerializer {
    public static string Serialize(object value)          // → 紧凑 JSON
    public static string SerializePretty(object value)    // → 2 空格缩进 JSON
    public static T      Deserialize<T>(string json)
    public static object Deserialize(Type type, string json)
}
```

### 成员规则

参与序列化的成员 = **public 非 static 字段** + **属性**（自动属性与计算属性都在内）；
private 字段、static 字段不参与。

| 特性 | 作用 | 标注位置 |
|---|---|---|
| `[JsonProperty("k")]` | 该成员在 JSON 中用键名 `k` | 字段、自动属性 |
| `[JsonIgnore]` | 序列化与反序列化都跳过该成员 | 字段、自动属性 |

未标注时键名 = 成员名，**大小写原样**（没有 camelCase / PascalCase 转换策略）。

> **计算属性（只有 getter 方法体、无背后字段）上的这两个特性不生效**——属性仍以成员名
> 出现在输出里，`[JsonIgnore]` 也不会把它排除。需要排除时改用别的成员形态。

只读属性（无 setter）只在序列化时写出，反序列化时跳过。

### 类型覆盖

| 目标类型 | ↔ JSON |
|---|---|
| `int` / `long` / `double` / `bool` / `string` | 对应基元 |
| 嵌套用户类 | object |
| 定长数组 `T[]` | array |
| `List<T>` | array |
| `Dictionary<string, V>` | object |
| `null` | `null` |

**数值按目标成员的静态类型落地**：JSON 里的 `1` 进 `double` 成员是 `1.0`，进 `int` 成员是
`1`。序列化方向则跟随对象的**运行期类型**，多态元素按其真实类型写出。

不支持（会抛异常或静默落空，别依赖）：enum、可空 `T?`、`char`、`Set` / `Queue` / `Stack`、
键不是 `string` 的 `Dictionary<K,V>`（抛 `JsonException`）、JSON Schema 校验。

### 反序列化的构造与绑定

- 类型**有无参构造器（或完全没有构造器）** → 先构造空对象，再按 JSON 键逐个赋给可写成员。
- 否则 → 选**参数最多**的构造器，按**参数名**匹配 JSON 键调用，构造后再补齐其余可写成员
  （`[Record]` 主构造器、只读自动属性走这条路）。
- **缺键** → 成员保持默认值；构造器参数取其默认值（可选参数）或按类型取 `null` 绑定。
- **多余键** → 忽略。
- **类型不匹配**（如字符串给 `int` 成员）→ `JsonException`。
- **JSON 顶层形状与目标类型不符**（如把 `[1,2]` 反序列化成一个类）→ **不报错**，得到一个
  成员全为默认值的对象。需要先判形状的场景，自己先 `JsonValue.Parse` 检查 kind。

## 低层公开类型

`JsonParser` / `JsonWriter` / `JsonMember` / `JsonMembers` 也是 public，但它们只是上面入口点
的载体，直接用没有额外能力，签名也不做稳定性承诺。日常代码用 `JsonValue` 与
`JsonSerializer` 两个入口即可。

## 用法

```z42
using Std;
using Std.IO;
using Std.Json;

class Point {
    public int X;
    public int Y;
    [JsonProperty("label_text")] public string Label;
    [JsonIgnore] public string Secret;
}

void Main() {
    // DOM：读
    JsonValue cfg = JsonValue.Parse("{\"api\":{\"users\":[{\"name\":\"bob\"}]}}");
    Console.WriteLine(JsonPath.Select(cfg, "$.api.users[0].name").AsString());   // bob

    // DOM：写
    JsonValue o = JsonValue.OfObject();
    o.Set("n", JsonValue.OfLong(1));
    JsonValue arr = JsonValue.OfArray();
    arr.Add(JsonValue.OfString("a"));
    o.Set("xs", arr);
    Console.WriteLine(JsonValue.Stringify(o));            // {"n":1,"xs":["a"]}

    // serde
    Point p = new Point();
    p.X = 3; p.Y = 4; p.Label = "hi"; p.Secret = "shh";
    Console.WriteLine(JsonSerializer.Serialize(p));        // {"X":3,"Y":4,"label_text":"hi"}
    Point back = JsonSerializer.Deserialize<Point>("{\"X\":7,\"label_text\":\"z\"}");
    Console.WriteLine(back.X.ToString() + " " + back.Label);   // 7 z

    // 工具配置：带注释和尾逗号
    JsonValue ts = JsonValue.ParseRelaxed("{ /* tsconfig */ \"strict\": true, }");
    Console.WriteLine(ts.Get("strict").AsBool().ToString());    // true

    // 错误定位
    try {
        JsonValue.Parse("{\"a\": }");
    } catch (JsonException e) {
        Console.WriteLine(e.ToString());                  // JsonException at 1:7: ...
    }
}
```

## 不支持

- **增量 / 流式解析**：`ParseStream` 只是「读到 EOF 再整体解析」，不是逐 token 的流式 reader。
- **注释保留**：relaxed 模式解析掉的注释不进值树，也不会被 `Stringify` 写回。
- **JSON Schema 校验**、自定义命名策略、自定义转换器。
- **`Stringify` 的两个非法输出**（读进来能原样写回去不成立的场景）：
  - `NaN` / `Infinity` 写出为 `NaN` / `inf` / `-inf`——不是合法 JSON，连 `ParseRelaxed`
    也读不回来（relaxed 只认 `NaN` / `Infinity` 拼写）。要 round-trip 得自己先检出这些值。
  - 整值 double（`42.0`）写出为 `42`，再解析回来 kind 变 Long。
- **数组下标 / 键缺失不返回默认值**：`At` / `Get` 越界或缺键一律抛异常，需要容错就先
  `ContainsKey` / `Length`，或改用 `JsonPath.Select`（走不通返回 `null`）。
