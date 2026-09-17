# z42.yaml —— YAML 读写

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.yaml/`；命名空间 `Std.Yaml`（异常类 `YamlException` 在 `Std`）

把 [YAML 1.2](https://yaml.org/spec/1.2.2/) 文本解析成值树（`YamlValue`），或把值树写成块风格
YAML。覆盖 Docker Compose / kubectl / Helm 这类配置日常用到的子集：块映射与块序列、flow 风格、
块标量、锚点与别名、merge key、类型 tag、多文档流、时间戳。

**两条最容易踩的边界**（详见[不支持](#不支持)）：

1. `- ` 序列项下面**跟多行的嵌套映射不支持**（`- name: bob` 换行后再写 `age: 3` 会报错）；
   写成 `-` 单独一行 + 下一层缩进的映射即可。
2. 复杂键（`? key` 语法）与 `%YAML` 指令不支持。

整份文档一次读进内存，解析 fail-fast（抛第一个错误）。

## YamlValue

值树的唯一节点类型，同时是包的入口点。8 种 kind：

| kind | `KindName()` | 载荷 |
|---|---|---|
| Null | `"null"` | —— |
| Bool | `"bool"` | `bool` |
| Int | `"int"` | `long`（i64） |
| Float | `"float"` | `double`（f64） |
| String | `"string"` | `string` |
| Sequence | `"sequence"` | 有序 `YamlValue` 列表 |
| Mapping | `"mapping"` | 有序 key → `YamlValue`（**键只能是字符串**） |
| Timestamp | `"timestamp"` | `Std.Time.DateTime` + 原始词法串 |

### 入口点

```z42
// 解析（Stream 重载读到 EOF，不关闭传入的 Stream）
public static YamlValue   Parse(string text)                 // 单文档；多文档输入报错
public static YamlValue[] ParseAll(string text)              // `---` 分隔的多文档
public static YamlValue   ParseStream(Std.IO.Stream source)
public static YamlValue[] ParseAllStream(Std.IO.Stream source)

// 序列化（块风格）
public static string Stringify(YamlValue root)
public static void   WriteTo(Std.IO.Stream dest, YamlValue root)
```

- 空 / 全空白输入：`Parse` 返回 **Null 值**，`ParseAll` 返回**空数组**。
- `Parse` 碰到第二个文档会抛
  `unexpected trailing content after document (use ParseAllDocuments for multi-doc YAML)`；
  文档尾的 `...` 标记允许出现。
- Stream 入口是**独立的名字**（`ParseStream` / `ParseAllStream`），不是 `Parse` 的重载——
  别去找 `Parse(Stream)`，没有。[JSON](json.md) / [TOML](toml.md) 两个包同样命名。

### 构造

```z42
public static YamlValue OfNull()
public static YamlValue OfBool(bool b)
public static YamlValue OfInt(long n)
public static YamlValue OfFloat(double d)
public static YamlValue OfString(string s)
public static YamlValue OfSequence()                     // 空序列，用 Add 填
public static YamlValue OfMapping()                      // 空映射，用 Set 填
public static YamlValue OfTimestamp(DateTime dt)         // Stringify 输出规范 UTC ISO 8601
public static YamlValue OfTimestampString(string iso)    // 保留原词法串；非法输入抛 ArgumentException
```

### 判别与取值

```z42
public bool IsNull()   public bool IsBool()     public bool IsInt()     public bool IsFloat()
public bool IsString() public bool IsSequence() public bool IsMapping() public bool IsTimestamp()
public string KindName()

public bool     AsBool()          // kind 不符 → YamlException("expected bool, got string")
public long     AsInt()           // 不接受 Float
public double   AsFloat()         // 不接受 Int
public string   AsString()
public DateTime AsTimestamp()
public string   AsTimestampRaw()  // 解析得来的原词法串；由 OfTimestamp 构造时为空串
```

`As*` 一律严格匹配 kind——`AsInt()` 不会把 Float 截断，`AsFloat()` 也不会提升 Int。

### 序列操作（kind == Sequence）

```z42
public int       Length()        // 非序列 → YamlException
public YamlValue At(int i)       // 越界 → YamlException（不是 VM abort）
public void      Add(YamlValue v)
public int       Count()         // 与 Length() 同值
```

### 映射操作（kind == Mapping）

```z42
public bool      ContainsKey(string key)   // 非映射值返回 false，不抛
public YamlValue Get(string key)           // 键不存在 → YamlException("key not found: k")
public void      Set(string key, YamlValue v)
public string[]  Keys()                    // 插入顺序快照
public int       Count()                   // 映射键数；序列则为元素数；其余 kind 抛
```

### DeepClone

```z42
public YamlValue DeepClone()
```

递归复制整棵子树。别名（`*name`）解析出来的值就是锚点值的 `DeepClone`——改动一个别名不会
影响锚点本体或其它别名。

## 支持的语法

### 标量推断

未加引号的标量按下表判定，**顺序自上而下**，都不匹配则是字符串：

| 写法 | 结果 |
|---|---|
| 空、`~`、`null` / `Null` / `NULL` | Null |
| `true` / `True` / `TRUE` / `false` / `False` / `FALSE` | Bool |
| `0xFF`（**小写 `x`**，无符号） | Int（十六进制） |
| `0o755`（**小写 `o`**，无符号） | Int（八进制） |
| `2026-05-27`、`2026-05-27T07:32:00Z`、`2026-05-27 07:32:00+08:00` | Timestamp（ISO 8601 前缀形状） |
| `42` / `-7` / `+7` | Int（十进制，可带符号） |
| `3.14` / `1.5e10`（必须有小数点或指数） | Float |
| 其余 | String |

刻意**按 YAML 1.2 判定**：`yes` / `no` / `on` / `off` 一律是**字符串**，没有 Norway problem。
`0XFF` / `0O755`（大写前缀）也是字符串。日期形状但 `DateTime.ParseIso8601` 解不开的串
（如 `2026-13-45`）回落为字符串 / 数字，不报错。

### 结构

| 语法 | 例子 |
|---|---|
| 块映射（缩进嵌套） | `a: 1`<br>`b:`<br>`  c: 2` |
| 块序列 | `- 1`<br>`- 2`；映射值下的序列可与键同缩进，也可更深缩进 |
| 单行内联映射作序列项 | `- name: bob`（**每个 `-` 只能带一对 k:v**） |
| flow 序列 / 映射 | `[1, 2]`、`{a: 1, b: [1, 2]}`、可嵌套 |
| 双引号字符串 | `"a\nb"`，转义 `\n \t \r \" \\ \/ \0 \uXXXX` |
| 单引号字符串 | `'don''t'`（只有 `''` → `'` 一个转义） |
| 块标量 literal | `key: \|` + 缩进内容，换行原样保留 |
| 块标量 folded | `key: >` + 缩进内容，连续非空行折成单个空格，空行折成换行 |
| chomping / 缩进指示符 | `\|-` strip、`\|+` keep、默认 clip；`\|2` 显式缩进（1-9） |
| 注释 | 整行 `# …`；行尾注释必须前面有空格（`a: 1 # x`） |
| 锚点 / 别名 | `base: &b` … `copy: *b`（**逐文档作用域**，别名取 `DeepClone`） |
| merge key | `<<: *b` / `<<: [*a, *b]` |
| 类型 tag | `!!str` `!!int` `!!float` `!!bool` `!!null` |
| 多文档 | `---` 分隔、`...` 结束标记（经 `ParseAll`） |

### 类型 tag 的细则

| 用法 | 结果 |
|---|---|
| `k: !!str 42` | 字符串 `"42"` |
| `k: !!int "42"` | Int `42`；解不开 → `YamlException` |
| `k: !!float "1.5"` | Float；解不开 → `YamlException` |
| `k: !!bool true` / `!!bool FALSE` | Bool（只认 true/True/TRUE/false/False/FALSE，`yes` 报错） |
| `k: !!null` / `!!null ~` / `!!null null` | Null；其它值报错 |
| `k: !myTag 5` / `k: !!binary x` | 未知 / 本地 tag **静默忽略**，退回标量推断（这里得 Int `5`） |

不支持 `!!seq` / `!!map` 集合 tag、`!<tag:yaml.org,2002:str>` 冗长 URI 形式、用户 tag 注册表，
以及同一位置同时写 tag 和锚点（`&a !!str 42`）。

### merge key 的细则

- 只有**未加引号**的 `<<` 触发合并；`"<<"` / `'<<'` 保持字面键，这是逃生口。
- 值必须解析成映射或「全是映射的序列」，否则抛带 `merge` 字样的 `YamlException`。
- **显式键永远压过合并来的键**，与书写位置无关；`<<: [*a, *b]` 中靠前的源优先。
- **浅合并**：显式键若是映射，整体替换合并来的映射，不做递归融合。
- `Stringify` 输出的是展开后的映射，**不会写回 `<<:`**。

### 解析期强制的规则

都抛 `YamlException`（消息尾部带 `at 行:列`）：

- 映射内重复键 → `duplicate mapping key 'a'`
- 缩进里出现制表符 → `tab character not allowed in indentation`
- 缩进层级对不上 → `unexpected indent at mapping level` / `... at sequence level`
- 未定义 / 前向引用的别名 → `undefined anchor: *x`
- 单双引号字符串里直接换行、未闭合的引号 / flow 括号
- 块或 flow 嵌套超过 **256** 层 → `nesting too deep (max 256)`

## Stringify 的输出形态

- 块风格、2 空格缩进，键按插入顺序。
- 映射值是非空序列时，`- ` 与父键**同缩进**；值是非空映射时缩进 +2。
- 空映射输出 `{}`，空序列输出 `[]`，Null 输出 `~`（作为映射值 / 序列项时输出为空）。
- 字符串保守加引号：会被重新读成别的 kind（`true` / `42` / `~` …）、含 YAML 结构指示符
  （`: # & * ! | > " ' , [ ] { } % @`、反引号、反斜杠、制表符与换行）、带首尾空白、或以
  `-` / `?` / `:` 开头的，一律双引号 + 转义；其余裸写。
- Timestamp 优先写回原词法串（`OfTimestampString` / 解析得来的），否则写规范 UTC ISO 8601。
- **不保留**：注释、原引号风格、flow 风格、锚点 / 别名 / merge key（别名展开成副本）。

## YamlException

```z42
namespace Std;
public class YamlException : Exception {
    public YamlException(string message)
}
```

与 `JsonException` / `TomlException` 不同，**它没有 `Line` / `Column` 字段**——解析错误的位置
拼在消息里（`"... at 3:1"`）。要按位置分流只能解析消息文本。

## 低层公开类型

`YamlParser` / `YamlWriter` 也是 public，但只是上面入口点的载体，直接用没有额外能力。日常
代码用 `YamlValue` 一个入口即可。

## 用法

```z42
using Std;
using Std.IO;
using Std.Yaml;

void Main() {
    string src = "name: alice\n"
        + "age: 30\n"
        + "friends:\n  - bob\n  - charlie\n"
        + "limits: {cpu: 2, mem: 512}\n";
    YamlValue v = YamlValue.Parse(src);
    Console.WriteLine(v.Get("name").AsString());                  // alice
    Console.WriteLine(v.Get("age").AsInt().ToString());           // 30
    Console.WriteLine(v.Get("limits").Get("cpu").AsInt().ToString());  // 2

    YamlValue friends = v.Get("friends");
    int i = 0;
    while (i < friends.Length()) {
        Console.WriteLine("- " + friends.At(i).AsString());
        i = i + 1;
    }

    // merge key：Docker Compose 的 x-common 写法
    string compose = "x-common: &common\n"
        + "  restart: unless-stopped\n"
        + "services:\n"
        + "  web:\n"
        + "    <<: *common\n"
        + "    image: nginx\n";
    YamlValue cfg = YamlValue.Parse(compose);
    Console.WriteLine(cfg.Get("services").Get("web").Get("restart").AsString());  // unless-stopped

    // 多文档（kubectl 清单栈）
    YamlValue[] docs = YamlValue.ParseAll("kind: Service\n---\nkind: Deployment\n");
    Console.WriteLine(docs.Length.ToString());                    // 2

    // 建树再写出
    YamlValue root = YamlValue.OfMapping();
    root.Set("enabled", YamlValue.OfBool(true));
    YamlValue seq = YamlValue.OfSequence();
    seq.Add(YamlValue.OfString("a"));
    root.Set("items", seq);
    Console.WriteLine(YamlValue.Stringify(root));
    // enabled: true
    // items:
    // - a

    try {
        YamlValue.Parse("a: 1\na: 2\n");
    } catch (YamlException e) {
        Console.WriteLine(e.Message);       // duplicate mapping key 'a' at 3:1
    }
}
```

## 不支持

- **序列项下的多行嵌套映射**——最常踩的一条。

  ```yaml
  items:
    - name: bob      # ✗ 下一行的 age 会报 "unexpected indent at sequence level"
      age: 3
  ```

  每个 `- ` 只支持**同一行内的一对 `k: v`**。多字段的写法是让 `-` 单独占一行：

  ```yaml
  items:
    -
      name: bob      # ✓
      age: 3
  ```

  同理，`- - 1` 这种同行嵌套序列也不支持，得换成 `-` 单独一行 + 下一层的块序列。
- **复杂键**（`? key` 后跟 `: value` 的语法）：映射键只能是字符串。
- **`%YAML` / `%TAG` 指令**：指令行会被当成文档内容，导致解析报错。
- **flow 上下文里的锚点声明**：`[*alias]` 可以用，`[&a 1, *a]` 里的 `&a 1` 会被当成普通标量，
  随后引用它的别名报 `undefined anchor`。
- **Stringify 的 flow 风格 / 注释保留 / 锚点回写**：输出恒为块风格的展开形态。
- **Schema 校验**、对象 ↔ YAML 的 serde（只有 `z42.json` 提供 serde 层）。
- **增量 / 流式解析**：`ParseStream` 只是「读到 EOF 再整体解析」。
- **多错误聚合**：解析在第一个错误处抛出，不返回错误列表。
