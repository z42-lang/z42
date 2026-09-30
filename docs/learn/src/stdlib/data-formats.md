# 数据格式

配置文件、接口返回、数据交换——这一章讲怎么把 JSON、TOML、YAML 读成程序里的值，再写回文本。

三个格式各有一个包，形状**刻意做成一样的**：

| 格式 | 命名空间 | 值树类型 | 典型用途 |
|---|---|---|---|
| JSON | `Std.Json` | `JsonValue` | 接口数据、机器之间交换 |
| TOML | `Std.Toml` | `TomlValue` | 工程配置（z42 自己的 `z42.toml` 就是它）|
| YAML | `Std.Yaml` | `YamlValue` | 人写的配置、多文档清单 |

三个值树的 API 长得几乎一样：`Parse` 解析、`Stringify` 写回、`Of*` 构造、`Is*` / `KindName`
判别、`As*` 取值、`Get` / `Set` / `Keys` 访问键、`TryGetValue<T>` 取即检查。学会一个，另两个
照着用。完整 API 见参考手册的
[JSON](https://z42-lang.github.io/z42/reference/stdlib/json.html)、
[TOML](https://z42-lang.github.io/z42/reference/stdlib/toml.html)、
[YAML](https://z42-lang.github.io/z42/reference/stdlib/yaml.html)。

> 三个包都随 SDK 发布，单文件 `z42 run x.z42` 里 `using Std.Json;` 就能用，不必建工程。
> 从 `Stream` 读的入口叫 `ParseStream`（不是 `Parse` 的重载）——三个包一致。

## JSON：值树

`Parse` 把文本变成一棵值树，`Get` 按键取子树，`As*` 取出标量：

```z42
// examples/stdlib/data-formats/json-dom/dom.z42
{{#include ../../../../examples/stdlib/data-formats/json-dom/dom.z42:parse}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-dom/run.console:parse}}
```

**`As*` 严格匹配 kind**：对着一个数字调 `AsString()` 会抛异常，不会悄悄转成 `"42"`。想先问
清楚就用 `KindName()` 或 `IsLong()` 这类判别方法。

数组用 `Length()` 和 `At(i)`：

```z42
// examples/stdlib/data-formats/json-dom/dom.z42
{{#include ../../../../examples/stdlib/data-formats/json-dom/dom.z42:array}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-dom/run.console:array}}
```

### 取即检查：`TryGetValue<T>`

配置里「键在就用它、不在就用默认值」是最常见的需求。写成 `ContainsKey` + `Get` 要把键写两遍，
`TryGetValue<T>` 一行就够：

```z42
// examples/stdlib/data-formats/json-dom/dom.z42
{{#include ../../../../examples/stdlib/data-formats/json-dom/dom.z42:tryget}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-dom/run.console:tryget}}
```

三条规则（三个包一致）：

- **键不在**：返回 `false`，且**不动**你传进去的变量——所以默认值写在变量初始化那里。
- **收者不是对象**：也返回 `false`，不抛。
- **键在但类型不符**：**照抛**。「键没配」是常态，「配错类型」是文件写错了——把后者吞掉会让
  笔误静默退回默认值。

> 为什么是 `bool TryGetValue(..., ref T value)` 而不是返回一个可空值：z42 的**值类型永不可空**，
> 所以取不到时没有「空」可返回。这和 `Int32.TryParse` 是同一套形状。

### 自己搭一棵树写出去

```z42
// examples/stdlib/data-formats/json-dom/dom.z42
{{#include ../../../../examples/stdlib/data-formats/json-dom/dom.z42:build}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-dom/run.console:build}}
```

`Stringify` 紧凑输出，`StringifyPretty` 两空格缩进。**键的顺序就是你写入的顺序**，
`Set` 覆盖已有键时也留在原位——所以 round-trip 不会把文件重排，diff 好看。

## JSON：直接和对象互转

值树适合结构不固定的数据。如果你已经有类，用 `JsonSerializer` 一步到位：

```z42
// examples/stdlib/data-formats/json-serde/serde.z42
{{#include ../../../../examples/stdlib/data-formats/json-serde/serde.z42:decl}}
```

```z42
// examples/stdlib/data-formats/json-serde/serde.z42
{{#include ../../../../examples/stdlib/data-formats/json-serde/serde.z42:write}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-serde/run.console:write}}
```

参与序列化的是**public 非 static 字段**和**属性**；private 字段、static 字段不参与。两个特性
可以调整：`[JsonProperty("k")]` 换键名，`[JsonIgnore]` 整个跳过（上面的 `Password` 就没出现）。

反过来用 `Deserialize<T>`：

```z42
// examples/stdlib/data-formats/json-serde/serde.z42
{{#include ../../../../examples/stdlib/data-formats/json-serde/serde.z42:read}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-serde/run.console:read}}
```

嵌套类、数组、`List<T>`、`Dictionary<string, V>` 都直接支持。**数值按目标成员的静态类型落地**：
JSON 里的 `1` 进 `double` 成员是 `1.0`，进 `int` 成员是 `1`。

JSON 与类的字段对不齐时很宽容：

```z42
// examples/stdlib/data-formats/json-serde/serde.z42
{{#include ../../../../examples/stdlib/data-formats/json-serde/serde.z42:partial}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/json-serde/run.console:partial}}
```

缺的键留零值 / `null`，多余的键忽略——都不报错。**需要「必填」语义就自己检查**，
序列化器不会替你把关。

> 🔴 **计算属性上的这两个特性不生效**：只有 getter 方法体、没有后备字段的属性（第 13 章的
> `Doubled` 那种），`[JsonProperty]` 换不了键名，`[JsonIgnore]` 也排除不掉——它照样以成员名
> 出现在输出里。要排除就别用计算属性这种形态。

## TOML：读工程配置

TOML 的根**永远是一张表**。嵌套用 `[section]`，重复的段用 `[[section]]`（解析出来是数组）：

```z42
// examples/stdlib/data-formats/toml/toml.z42
{{#include ../../../../examples/stdlib/data-formats/toml/toml.z42:src}}
```

```z42
// examples/stdlib/data-formats/toml/toml.z42
{{#include ../../../../examples/stdlib/data-formats/toml/toml.z42:read}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/toml/run.console:read}}
```

配置场景正是 `TryGetValue<T>` 的主场：

```z42
// examples/stdlib/data-formats/toml/toml.z42
{{#include ../../../../examples/stdlib/data-formats/toml/toml.z42:tryget}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/toml/run.console:tryget}}
```

> TOML 的整数是 64 位，所以标量整数用 `TryGetValue<long>`；要落进 `int` 自己窄化一次。

两个和 JSON 不同的地方：

```z42
// examples/stdlib/data-formats/toml/toml.z42
{{#include ../../../../examples/stdlib/data-formats/toml/toml.z42:quirks}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/toml/run.console:quirks}}
```

- ⚠️ **`AsDouble()` 不接受整数**。JSON 的 `AsDouble()` 会把 Long 提升成 double，TOML 的不会——
  `port = 8080` 只能用 `AsLong()` 取。
- **空输入解析出空表**，不是 `null`，所以「配置文件是空的」不需要单独判。

写回用 `Stringify`（根必须是表）：

```z42
// examples/stdlib/data-formats/toml/toml.z42
{{#include ../../../../examples/stdlib/data-formats/toml/toml.z42:write}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/toml/run.console:write}}
```

> 🔴 **TOML 的 4 种 datetime 类型还不支持**——写了日期会报解析错误（消息里会说这件事）。
> 暂时需要日期就用引号当字符串带过去。

## YAML：人写的配置

YAML 支持缩进嵌套、序列、多文档、块标量。kind 比另两个多两种：`null` 和 **`timestamp`**
（YAML 有日期，TOML 没有）。

```z42
// examples/stdlib/data-formats/yaml/yaml.z42
{{#include ../../../../examples/stdlib/data-formats/yaml/yaml.z42:src}}
```

```z42
// examples/stdlib/data-formats/yaml/yaml.z42
{{#include ../../../../examples/stdlib/data-formats/yaml/yaml.z42:read}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/yaml/run.console:read}}
```

> ⚠️ 取值器的名字这里**不一样**：YAML 是 `AsInt()` / `AsFloat()`，而 JSON 和 TOML 是
> `AsLong()` / `AsDouble()`。三个包只有这一处名字对不齐。

### `no` 是字符串，不是 `false`

不加引号的标量按一张表推断类型。z42 刻意**按 YAML 1.2 判定**：

```z42
// examples/stdlib/data-formats/yaml/yaml.z42
{{#include ../../../../examples/stdlib/data-formats/yaml/yaml.z42:norway}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/yaml/run.console:norway}}
```

只有 `true` / `false`（及其大写形式）是布尔，`yes` / `no` / `on` / `off` 一律是**字符串**。
这躲掉了 YAML 1.1 那个著名的坑——挪威的国家代码 `NO` 被读成布尔假（所谓 "Norway problem"）。

### 多文档

一个 YAML 文件可以用 `---` 分隔多份文档（Kubernetes 清单常这么写）。`Parse` 只收单份，
多份要用 `ParseAll`：

```z42
// examples/stdlib/data-formats/yaml/yaml.z42
{{#include ../../../../examples/stdlib/data-formats/yaml/yaml.z42:multidoc}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/yaml/run.console:multidoc}}
```

拿 `Parse` 去读多文档会报错，消息里会告诉你改用 `ParseAll`。

### 块标量

`|` 保留换行，`>` 把连续的行折成空格——写脚本、写长文本时用：

```z42
// examples/stdlib/data-formats/yaml/yaml.z42
{{#include ../../../../examples/stdlib/data-formats/yaml/yaml.z42:block}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/yaml/run.console:block}}
```

## 出错时抛什么

三个包各有自己的异常类型，都是解析到第一个错误就停（不会攒一堆错误再报）：

```z42
// examples/stdlib/data-formats/errors/errors.z42
{{#include ../../../../examples/stdlib/data-formats/errors/errors.z42:types}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/errors/run.console:types}}
```

⚠️ **位置信息的拿法不一致**：`JsonException` 和 `TomlException` 有 `Line` / `Column` 字段，
而 **`YamlException` 没有**——它把位置拼在消息末尾（`... at 2:1`）。

取值出错也是同一族异常：

```z42
// examples/stdlib/data-formats/errors/errors.z42
{{#include ../../../../examples/stdlib/data-formats/errors/errors.z42:kind}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/errors/run.console:kind}}
```

```z42
// examples/stdlib/data-formats/errors/errors.z42
{{#include ../../../../examples/stdlib/data-formats/errors/errors.z42:nodate}}
```

```console
{{#include ../../../../examples/stdlib/data-formats/errors/run.console:nodate}}
```

## 三个格式的差异速查

| | JSON | TOML | YAML |
|---|---|---|---|
| 根 | 任意值 | **永远是表** | 任意值 |
| 空输入 | 解析错误 | **空表** | Null 值 |
| 注释 | 严格模式不支持（`ParseRelaxed` 支持）| `#` | `#` |
| 尾逗号 | 同上 | 数组可以 | — |
| 日期 | 无（自己用字符串）| 🔴 **不支持**（报错）| **Timestamp** |
| 整数取值器 | `AsLong()` | `AsLong()` | **`AsInt()`** |
| `AsDouble` 接受整数 | ✅ | ❌ | ❌（`AsFloat()` 不接受 Int）|
| 多文档 | — | — | `ParseAll` |
| 对象 ↔ 类 | **`JsonSerializer`** | 手写 | 手写 |

选哪个：**机器之间交换用 JSON**（还有 serde 可以省掉手写映射）；**工程配置用 TOML**
（结构扁平、没有缩进歧义）；**人要手写且结构深、或者要多文档就用 YAML**。

> JSON 还有一个 `ParseRelaxed`，额外接受 `//` 注释、块注释、尾逗号、`NaN` / `Infinity`——
> 用来读 `tsconfig.json` 这类「带注释的 JSON」。严格模式（`Parse`）完全按 RFC 8259，不多不少。

## 小结

- 三个包形状一致：`Parse` / `Stringify` / `Of*` / `Is*` / `KindName` / `As*` / `Get` / `Set` /
  `Keys` / `TryGetValue<T>`；从流读用 `ParseStream`。都随 SDK 发布，单文件模式直接可用。
- `As*` **严格匹配 kind**，不做隐式转换；`Get` 缺键抛异常，`At` 越界抛异常（不是崩溃）。
- `TryGetValue<T>`：键不在返回 `false` 且**不动变量**，收者不是对象也返回 `false`，
  **键在但类型不符照抛**。配置读取的标准写法。
- 键顺序 = 写入顺序，`Set` 覆盖保持原位 ⇒ round-trip 不重排。
- `JsonSerializer` 直接和类互转：public 字段与属性参与，`[JsonProperty]` 换键名、
  `[JsonIgnore]` 跳过；缺键给零值、多余键忽略（**「必填」要自己检查**）。
  🔴 计算属性上这两个特性不生效。
- TOML：根恒为表、空输入是空表、`AsDouble()` **不**接受整数、🔴 **datetime 不支持**。
- YAML：取值器叫 `AsInt()` / `AsFloat()`；`no` / `yes` / `on` / `off` 是**字符串**（YAML 1.2，
  没有 Norway problem）；多文档用 `ParseAll`；有 `timestamp` kind。
- 异常：`JsonException` / `TomlException` 带 `Line` / `Column`，⚠️ **`YamlException` 没有**
  （位置在消息里）。

下一章讲**文本处理**——`StringBuilder` 与正则表达式。
