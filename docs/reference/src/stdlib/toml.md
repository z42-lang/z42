# z42.toml —— TOML 读写

> 包路径 `src/libraries/z42.toml/`；命名空间 `Std.Toml`（含异常类 `TomlException`）

把 [TOML 1.0](https://toml.io/en/v1.0.0) 文本解析成值树（`TomlValue`），或把值树写回 canonical
TOML 文本。z42 自己的工程清单 `*.z42.toml` 与 `versions.toml` 就用它读。

覆盖 TOML 1.0 的**绝大部分**语法，**唯一缺的大块是 4 种 datetime 类型**——碰到日期字面量会
报解析错误，不会静默当字符串吞掉。整份文档一次读进内存，解析 fail-fast（抛第一个错误）。

## TomlValue

值树的唯一节点类型，同时是包的入口点。6 种 kind：

| kind | `KindName()` | 载荷 |
|---|---|---|
| String | `"string"` | `string` |
| Long | `"long"` | `long`（i64） |
| Double | `"double"` | `double`（f64） |
| Bool | `"bool"` | `bool` |
| Array | `"array"` | 有序 `TomlValue` 列表 |
| Table | `"table"` | 有序 key → `TomlValue` |

### 入口点

```z42
// 解析：根永远是 Table；空输入 → 空 Table（不是 null）
public static TomlValue Parse(string text)
public static TomlValue ParseStream(Std.IO.Stream source)   // UTF-8 读到 EOF，不关闭 Stream

// 序列化：root 必须是 Table，否则抛 TomlException
public static string Stringify(TomlValue root)
public static void   WriteTo(Std.IO.Stream dest, TomlValue root)
```

`ParseStream` 是**独立的名字**，不是 `Parse` 的重载——别去找 `Parse(Stream)`，没有。
[JSON](json.md) / [YAML](yaml.md) 两个包同样命名。

### 构造

```z42
public static TomlValue OfString(string s)
public static TomlValue OfLong(long n)
public static TomlValue OfDouble(double d)
public static TomlValue OfBool(bool b)
public static TomlValue OfArray()    // 空数组，用 Add 填
public static TomlValue OfTable()    // 空表，用 Set 填
```

### 判别与取值

```z42
public bool IsString() public bool IsLong() public bool IsDouble()
public bool IsBool()   public bool IsArray() public bool IsTable()
public string KindName()

public string AsString()   // kind 不符 → TomlException("expected string, got table")
public long   AsLong()
public double AsDouble()   // 不接受 Long；整数值要先自己转
public bool   AsBool()
```

### 表操作（kind == Table）

```z42
public bool      ContainsKey(string key)   // 非表值返回 false，不抛
public TomlValue Get(string key)           // 键不存在 → TomlException("key not found: k")
public void      Set(string key, TomlValue v)
public string[]  Keys()                    // 插入顺序快照
public int       Count()                   // 表键数；数组则为元素数；其余 kind 抛

// 取即检查（取值与存在性测试合成一次）
public bool      TryGet(string key, ref TomlValue value)
public bool      TryGetValue<T>(string key, ref T value)
```

#### 取即检查：`TryGet` / `TryGetValue<T>`

`ContainsKey` + `Get` 是**两次调用**，键要写两遍，而「ContainsKey 为真 ⇒ Get 不抛」这件事
编译器看不见。`TryGet` 把两步合成一步：

```z42
// 旧写法：键写两遍，查两遍
if (p.ContainsKey("name")) { name = p.Get("name").AsString(); }

// 取即检查：一行，键一遍
p.TryGetValue<string>("name", ref name);
```

三条规则：

- **未命中返回 false 且不动 `value`** ⇒「键在就覆盖、不在就保留默认」是主用法，一行写完。
- **收者不是表时返回 false 而不抛** —— 与 `ContainsKey` 一致，所以老写法可以逐字替换。
- **命中但类型不符照抛** `TomlException`，与 `AsString()` / `AsLong()` 一字不差。
  「键不在」和「值类型错」是两种失败：前者是常态，后者是文件写错了，吞掉它会让
  配置里的类型笔误**静默退回默认值**。

`T` 取 `string` / `long` / `double` / `bool` / `TomlValue`（原样取子树）。其它类型抛异常 ——
那是调用方写错了代码，返回 false 会把它伪装成「键不在」。

> TOML 整数是 i64 ⇒ 标量整数用 `<long>`；要落进 `int` 自己窄化一次。
> 返回 `bool` 而不是可空值，是因为**值类型永不可空**（[E0476](../appendix/error-codes.md)）——
> 与 `Int32.TryParse` 同一套 `bool TryX(ref T)` 形状。

`Std.Json.JsonValue` 与 `Std.Yaml.YamlValue` 有**同名同形**的一对。

`Keys()` 与 `Stringify` 都按**插入顺序**（解析出来的树即文件顺序），`Set` 覆盖已有键时保持
原位置——round-trip 不重排，diff 友好。

### 数组操作（kind == Array）

```z42
public int       Length()        // 非数组 → TomlException
public TomlValue At(int i)       // 越界 → TomlException（不是 VM abort）
public void      Add(TomlValue v)
public int       Count()         // 与 Length() 同值
```

## 支持的语法

| 语法 | 例子 | 备注 |
|---|---|---|
| 裸键 / 引号键 | `a-b_1 = 1`、`"a.b" = 1`、`'lit key' = 1` | 裸键字符集 `[A-Za-z0-9_-]` |
| 点分键 | `a.b.c = true` | 自动建中间表 |
| 表头 | `[section]`、`[a.b.c]` | 中间表隐式创建；`[a.b]` 先于 `[a]` 合法 |
| 表数组 | `[[exe]]` ... `[[exe]]`、`[exe.sub]` | 后续 `[x.sub]` 落到最后一个元素上 |
| 基本字符串 | `"a\tb"` | 转义 `\" \\ \n \t \r \0 \b \f \uXXXX \UXXXXXXXX` |
| 字面字符串 | `'C:\raw\path'` | 无转义 |
| 多行基本字符串 | `"""…"""` | 全套转义 + `\` 行尾续行折叠；紧跟开引号的换行被裁掉 |
| 多行字面字符串 | `'''…'''` | 完全逐字；同样裁掉首个换行 |
| 整数 | `42`、`-7`、`1_000_000` | i64；`_` 必须夹在两个数字之间 |
| 非十进制整数 | `0xDEAD_beef`、`0o755`、`0b1010` | 小写前缀，不允许带正负号 |
| 浮点 | `3.14`、`1.5e10`、`inf`、`+inf`、`-inf`、`nan` | f64 |
| 布尔 | `true` / `false` | |
| 数组 | `[1, 2, [3, 4]]`、跨行 + 行内注释 + 尾逗号 | **允许异构**：`[1, "x", true]` |
| 内联表 | `{ a = 1, b = "x" }` | 不允许尾逗号（符合 TOML 1.0） |
| 注释 | `# 整行`、`k = 1  # 行尾` | |

Unicode 转义走 z42 的 32-bit `char`，`\U0001F600` 这类增补平面码点直接可用；超过 `U+10FFFF`
报错。

**解析期强制的规则**（都抛 `TomlException`，带 1-based 行列）：

- 重复键 `k = 1` / `k = 2` → `duplicate key: k`
- 重复表头 `[a]` 两次 → `duplicate table definition: [a]`；`[[a]]` 与 `[a]` 互冲也报错
- 十进制整数前导零 `042` → `invalid number: leading zeros not allowed`
- `_` 不在两数字之间 → `underscore separator must ...`
- 单行字符串里出现换行 → 提示改用 `"""`
- 嵌套（数组 / 内联表）超过 **256** 层 → `nesting too deep (max 256)`

## Stringify 的输出形态

- root 表的非表值先按插入顺序输出 `key = value`，随后逐个子表输出 `[a.b]` 段、表数组输出
  `[[x]]` 段，递归。
- 键：全是 `[A-Za-z0-9_-]` 则裸键，否则加双引号转义。
- 数组恒为单行 `[a, b, c]`；元素全是表的非空数组按 `[[x]]` 形式展开。
- 整值浮点补出小数点（`42.0`），保证再解析回来仍是 Double。
- **内联表不会原样保留**：值树里的表一律输出成 `[section]` 段（数组元素里的表除外，那里输出
  `{ k = v }`）。
- **注释、空行、原始引号风格不保留**——`Stringify` 产出的是 canonical 形态，不是原文。

## TomlException

```z42
namespace Std;
public class TomlException : Exception {
    public int Line;      // 1-based；0 = 位置不可用（如 stringify 阶段错误）
    public int Column;
    public TomlException(string message)
    public TomlException(string message, int line, int column)
    override string ToString()   // "TomlException at 2:6: duplicate key: k"
}
```

## 低层公开类型

`TomlParser` / `TomlWriter` 也是 public，但只是上面入口点的载体，直接用没有额外能力。日常
代码用 `TomlValue` 一个入口即可。

## 用法

```z42
using Std;
using Std.IO;
using Std.Toml;

void Main() {
    // 读清单
    TomlValue manifest = TomlValue.Parse(File.ReadAllText("versions.toml"));
    string rust = manifest.Get("build").Get("rust").AsString();
    Console.WriteLine(rust);

    // 可选键：取即检查
    TomlValue deps = null;
    if (manifest.TryGet("dependencies", ref deps)) {
        string[] names = deps.Keys();
        int i = 0;
        while (i < names.Length) {
            Console.WriteLine(names[i] + " = " + deps.Get(names[i]).AsString());
            i = i + 1;
        }
    }

    // 建树再写出
    TomlValue root = TomlValue.OfTable();
    root.Set("name", TomlValue.OfString("demo"));
    TomlValue sub = TomlValue.OfTable();
    sub.Set("enabled", TomlValue.OfBool(true));
    root.Set("opts", sub);
    Console.WriteLine(TomlValue.Stringify(root));
    // name = "demo"
    //
    // [opts]
    // enabled = true

    // 错误定位
    try {
        TomlValue.Parse("k = 1\nk = 2\n");
    } catch (TomlException e) {
        Console.WriteLine(e.ToString());        // TomlException at 2:6: duplicate key: k
    }
}
```

## 不支持

- **datetime（TOML 1.0 的 4 种日期时间类型）**：`d = 1979-05-27T07:32:00Z` 与 `d = 1979-05-27`
  都在 `1979` 之后报 `expected newline or EOF after value`。需要日期就先按字符串存
  （`d = "1979-05-27"`），读出来自己解析。
- **注释保留 / 原格式保留**：解析丢掉注释与空行，`Stringify` 不还原。
- **输出格式可调**：只有一种 canonical 形态，没有缩进 / 数组换行 / pretty 选项。
- **Schema 校验**、默认值填充、类型强制。
- **删除键**：`TomlValue` 只有 `Set`，没有 remove；要去键就重建表。
- **多错误聚合**：解析在第一个错误处抛出，不返回错误列表。
- **`nan` 的 round-trip**：`nan` 能解析，但 `Stringify` 写出的是 `NaN.0`，再解析会失败。有
  NaN 的值树别指望写回去还能读。（`inf` / `-inf` 的 round-trip 正常。）
