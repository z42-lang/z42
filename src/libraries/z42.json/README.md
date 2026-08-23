# z42.json

## 职责
JSON RFC 8259 reader / writer + **对象 ↔ JSON serde**（`JsonSerializer`）。DOM 层覆盖 7 个 value 类型（null / bool / number / string / array / object），含完整 string escape 集合（\b \f \n \r \t \" \\ \/ \uXXXX BMP + \uXXXX\uXXXX surrogate pair），compact + pretty 两种 stringify 模式。serde 层经反射把用户对象与 JSON 互转。

## 功能索引
| 功能 | 入口 / 文件 |
|------|-----------|
| DOM Parse / Stringify | `JsonValue.z42` 的 `Parse` / `Stringify` / `StringifyPretty` |
| DOM 构造 / 谓词 / 访问 | `JsonValue.z42` 的 `Of*` / `Is*` / `As*` / `Get`/`Set`/`At`/`Add` |
| JSONPath 选择 | `JsonPath.z42` 的 `Select(root, "$.a.b[0]")` |
| **对象 → JSON** | `JsonSerializer.z42` 的 `Serialize` / `SerializePretty` |
| **JSON → 对象** | `JsonSerializer.z42` 的 `Deserialize<T>` / `Deserialize(Type, json)` |
| **成员改名 / 忽略** | `[JsonProperty("k")]` / `[JsonIgnore]`（字段 + 属性均生效，含计算属性） |

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/JsonValue.z42` | 值类型 + 公开入口（`Parse` / `Stringify` / `StringifyPretty` / `Of*` 工厂 / `Is*` / `As*` / 容器操作） |
| `src/JsonException.z42` | `JsonException : Std.Exception`，带 1-based line / column |
| `src/JsonParser.z42` | 内部 recursive-descent parser + tokenizer，含 surrogate-pair 处理 |
| `src/JsonWriter.z42` | 内部 stringifier（compact + pretty 两模式） |
| `src/JsonPath.z42` | `static class JsonPath` — `Select(root, "$.api.users[0].name")` 嵌套访问；支持 `.name` / `[N]` / `["key"]` |
| `src/JsonSerializer.z42` | 公开 serde API（`Serialize` / `SerializePretty` / `Deserialize<T>` / `Deserialize(Type,...)`）+ 序列化核 `_toJson` |
| `src/JsonBinder.z42` | 反序列化反射核 `_fromJson`（按目标 Type 递归）+ 构造绑定（无参 ctor / 带参 ctor）|
| `src/JsonMember.z42` | 字段 + 属性统一成员抽象 + 键名/忽略解析（`JsonMembers.For`）|
| `src/JsonReflect.z42` | serde 反射底座（`__array_*` / `__property_custom_attributes` 的库自有 extern；自举 gen0-safe，见文件头注）|
| `src/JsonPropertyAttribute.z42` / `src/JsonIgnoreAttribute.z42` | `[JsonProperty("name")]` / `[JsonIgnore]` |

## 入口点
- `Std.Json.JsonValue.Parse(text)` → `JsonValue` (any root value)
- `Std.Json.JsonValue.Stringify(v)` → `string` (compact)
- `Std.Json.JsonValue.StringifyPretty(v)` → `string` (2-space indent)
- `Std.Json.JsonValue.{OfNull, OfBool, OfLong, OfDouble, OfString, OfArray, OfObject}` 构造器
- `Std.Json.JsonValue.{IsNull, IsBool, IsLong, IsDouble, IsString, IsArray, IsObject, KindName}` 谓词
- `Std.Json.JsonValue.{AsBool, AsLong, AsDouble, AsString}` 解构
- `Std.Json.JsonValue.{Get, Set, ContainsKey, Keys}` 对象操作
- `Std.Json.JsonValue.{Length, At, Add, Count}` 数组操作
- `Std.JsonException` 异常类（`Line` / `Column` 字段）

## 用法

```z42
using Std.Json;
using Std;

var v = JsonValue.Parse("{\"name\":\"foo\",\"a\":[1,2,3]}");
v.Get("name").AsString();      // "foo"
v.Get("a").Length();           // 3
v.Get("a").At(1).AsLong();     // 2

var root = JsonValue.OfObject();
root.Set("users", JsonValue.OfArray());
root.Get("users").Add(JsonValue.OfString("alice"));
JsonValue.Stringify(root);              // compact: {"users":["alice"]}
JsonValue.StringifyPretty(root);        // 2-space indented
```

### serde（对象 ↔ JSON）

```z42
class User {
    [JsonProperty("user_name")] public string Name { get; set; }
    public int Age { get; set; }
    [JsonIgnore] public string Password { get; set; }
    public int[] Scores;
}

User u = ...;
string json = JsonSerializer.Serialize(u);          // {"user_name":...,"Age":...,"Scores":[...]}（Password 被忽略）
User back = JsonSerializer.Deserialize<User>(json);  // 反射构造 + 按名回填（user_name → Name）
```

类型覆盖：基元（int/long/double/bool/string）+ 嵌套对象 + 定长数组 `T[]`。构造：有无参 ctor →
`Activator` + 按名 `SetValue`；否则按参数名映射 JSON 键 `ConstructorInfo.Invoke`（record / 只读
auto-prop）。List/Dictionary/enum/nullable 见 roadmap Deferred。

## 依赖关系
依赖 `z42.core` + `z42.text`（StringBuilder for stringify 缓冲）。无其他 stdlib 依赖。

## 与 z42.toml 的关系
两个包并用没问题（仅在 fix-instance-method-binding-receiver-aware 修复后正确）—— 之前因为 method-name dispatch bug 互相干扰，已在 2026-05-15 修复。
