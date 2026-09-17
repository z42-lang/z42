# 命名约定

> 对齐：2026-09-17

z42 用户代码的命名约定。**绝大多数是强建议**——违反不会编译失败，但 stdlib、工具链与
`z42-fmt` 默认风格按此为准。**三条例外是编译期硬规则**（见 [§14](#14-后缀强制三条编译期硬规则)）。

设计目标：让代码"看上去就知道是什么"——类型 vs 值、公开 vs 私有、字段 vs 局部，
**零歧义零思考**。规范从 C# / Rust / Swift / Go / Python (PEP 8) 各取所长。

## 速查表

| 标识符类型 | 约定 | 示例 |
|------|------|------|
| **类 / 结构体 / record / enum** | `PascalCase` | `class Calculator`, `enum Direction`, `record Point` |
| **Primitive 类型 struct（stdlib）** | BCL `PascalCase`（`Int32 / Boolean / SByte / …`）；keyword（`int / bool / i8 / …`）是源码层别名 | `public struct Int32 : …` |
| **接口** | `I` + `PascalCase` | `interface IDisposable`, `IEnumerable<T>` |
| **委托类型** | `PascalCase` | `delegate void OnClick(…)`, `Action<int>` |
| **方法 / 函数** | `PascalCase` | `void Main()`, `int Add(int a, int b)` |
| **属性 / 公开字段** | `PascalCase` | `public int Count`, `public string Name` |
| **私有字段** | `_` + `camelCase` | `private int _count`, `private string _name` |
| **公开静态字段 / 方法** | `PascalCase`（同实例公开成员）| `public static int MaxRetries`, `Math.Sqrt(x)` |
| **私有静态字段** | `_` + `camelCase`（同实例私有）| `private static int _instanceCount` |
| **局部变量 / 参数** | `camelCase` | `var index = 0`, `void f(int leftValue)` |
| **常量（`const`）/ 静态只读** | `PascalCase` | `const double Pi = 3.14`, `static readonly DateTime Epoch` |
| **泛型类型参数** | 单字母 `T` 或 `T` + 语义名 | `class Box<T>`, `Dictionary<TKey, TValue>` |
| **命名空间** | `PascalCase` 点分 | `Std.IO`, `Demo.Web.Api` |
| **目录名** | `PascalCase` | `src/Collections/`, `src/Exceptions/` |
| **文件名** | 与主类型同名 `PascalCase.z42` | `Calculator.z42`, `IDisposable.z42` |
| **Bool 方法 / 字段** | `Is*` / `Has*` / `Can*` / `*ed` / `*able` | `IsEmpty`, `HasValue`, `CanWrite`, `IsParsed` |
| **方法动词** | `Get/Set/Find/Add/Remove/Open/Close/Try/Parse/To/As/…` | `int.Parse(s)`, `list.ToArray()` |
| **Exception 类型** | `*Exception` 后缀（约定，不强制）| `TomlException`, `ArgumentException` |
| **Attribute / Analyzer / Generator 类型** | 同名后缀，**编译期强制** | `RouteAttribute`, `NoGotoAnalyzer` |
| **Enum** | 互斥单数 / 位标志复数；成员 PascalCase 不带前缀 | `enum Direction { North, … }` |
| **工厂方法** | `static Create*` / `From*` / `Empty()` / `Default` | `Color.FromHex("…")` |
| **Lambda / 解构 discard** | `_` 表忽略（不能后续引用）| `(_, y) = pair;` |

> **包名**（manifest `[project] name`）是分发层标识符，不是语言标识符，规则另见
> [工程文件规范](../toolchain/z42-toml.md#包名命名规则)。

---

## 1. 视觉层级：类型 vs 值

**类型大写开头，值小写开头**。一眼看出哪是类型哪是值，无需 IDE 高亮：

```z42
var greeter = new Greeter();   // Greeter 大写 = 类型；greeter 小写 = 实例
int count = list.Count();      // int = 内置类型；count = 值；Count = 方法
```

借鉴 Rust（`fn foo() -> Foo` 中 `foo` 与 `Foo` 的视觉区分），同时保留 C# 风格的
`PascalCase` 用于"东西的名字"。

**Primitive 类型例外**：源码层有两种等价拼写：

```z42
int x = 5;          // keyword 形式（推荐 —— 日常代码用）
Int32 y = 5;        // BCL 形式（等价；同 C# `int` ⟷ `System.Int32`）

x.ToString();       // 派发到 Std.Int32.ToString
Int32.Parse("42");  // 等价 int.Parse("42")
```

stdlib 的 struct 名采用 BCL PascalCase（`Boolean` / `Char` / `SByte` / `Int16` / `Int32` /
`Int64` / `Byte` / `UInt16` / `UInt32` / `UInt64` / `Single` / `Double`），文件名 = struct 名。
z42 关键字（`int` / `long` / `bool` / `char` / `float` / `double` / `i8` / `i16` / `u8` /
`u16` / `u32` / `u64` …）是源码层别名，编译期归一到同一个类型。

| keyword | struct |
|---------|:------:|
| `int` | `Int32` |
| `long` | `Int64` |
| `i8` | `SByte` |
| `bool` | `Boolean` |
| `float` | `Single` |
| `double` | `Double`（拼写一致）|
| `char` | `Char`（拼写一致）|

## 2. 可见性靠关键字，不靠命名

Go 用大小写编码可见性（`Foo` exported / `foo` unexported）有简洁性优势，但代价是限制了
**值标识符的命名自由**——比如 Go 必须写小写的 `func main()`。

z42 走 C# / Java / Kotlin 路线：**可见性由 `public` / `private` / `protected` / `internal`
关键字显式声明**，命名约定与可见性正交：

- 公开方法 `public int Sum()` 与私有方法 `private int Sum()` 都用 `PascalCase`；
- 公开字段 `public int Count` 与私有字段 `private int _count` 形状一致，靠 `_` 区分（见 §4）。

## 3. `I` 前缀保留给接口

C# 的 `I` 前缀（`IDisposable` / `IEnumerable`）**保留**。Swift / Kotlin / Rust trait 不带前缀
（`Comparable` / `Hashable` / `Iterator`），z42 选 C# 是因为：

- stdlib 已建立此约定（`IDisposable` / `IComparable<T>` / `IEnumerable<T>` / `INumber<T>` /
  `ISubscription<TD>` 等）；
- 接口与类在使用点的语法差异不显然（`new T()` 不能用于接口），名字前的 `I` 提供即时识别。

> **不要用 `T` 前缀或 `Interface` 后缀给接口**——`I` 前缀是明确选择。

**已知例外**：编译器自身的 analyzer / codefix 契约接口
（`DiagSink` / `Analyzer` / `FixSink`）**不带 `I` 前缀**。其中 `Analyzer` 是被迫的——
它的实现类受 `Analyzer` 后缀强制（§14），接口自己再叫 `IAnalyzer` 会让"接口名"与
"强制后缀"割裂。用户代码写自定义 analyzer 时会直接接触这三个名字。

## 4. 字段：公开 PascalCase，私有 `_camelCase`

```z42
public class Vector3 {
    public double X;       // 公开字段，PascalCase（与属性等价命名）
    public double Y;
    public double Z;

    private int _refCount; // 私有字段，下划线前缀 + camelCase
    private bool _isDirty;
}
```

**为什么私有字段用下划线**：

- 与局部变量 / 参数（同样 camelCase）区分。方法体内 `this._refCount` 与 `refCount`（参数）
  一眼可辨；
- C# 主流公约，z42 stdlib 已落地；
- 比 `m_` / `s_`（匈牙利命名的退化形式）更简洁。

> **不允许** `_` 前缀用于公开字段或方法——下划线前缀 ≡ "私有 / 内部"。

## 5. 局部变量 / 参数：camelCase

```z42
void ProcessOrder(int orderId, string customerName) {   // 参数 camelCase
    var totalAmount = 0;          // 局部 camelCase
    var itemCount = 0;
    foreach (var item in items) { // 'item' 简短局部
        totalAmount += item.Price;
        itemCount++;
    }
}
```

参数和局部变量同形状，因为它们的**生命周期一致**（栈作用域）。无下划线，无 PascalCase。

## 6. 泛型类型参数

- **单参数用 `T`**：`class List<T>`、`T Identity<T>(T x)`；
- **多参数用 `T` + 语义名**：`Dictionary<TKey, TValue>`、`Func<TInput, TOutput>`、
  `MulticastFunc<TArg, TResult>`；
- **不要用单字母 `K` / `V` / `E` / `R`**（Rust / Java 风格）—— z42 用
  `TKey` / `TValue` / `TElement` / `TResult`。

单字母 `T` 易识别为"类型参数"，但多个并列单字母（`Map<K, V>`）易混淆；语义前缀 `T` 让
"这是类型参数"显式。

```z42
class Cache<TKey, TValue> { … }
TResult Map<TInput, TResult>(TInput x, Func<TInput, TResult> f) { … }
```

## 7. 常量：PascalCase

```z42
class Config {
    public const double Pi      = 3.14159265;   // const 隐式 static，不要再写 static
    public const int MaxRetries = 3;
}
```

**不使用 `SCREAMING_SNAKE_CASE`**：

- Rust / Java / Python 用 `MAX_VALUE` 风格强调"编译时常量"；
- z42 跟随 C# / Swift 公约——常量是"值"，但**命名层级与类型对齐为 PascalCase**，因为：
  - 与同模块的 PascalCase 方法、类型保持单一视觉风格；
  - `Math.Pi` 比 `Math.PI` 在点分访问时更自然；
  - 可变性靠 `const` 关键字表达，不靠大小写。

> **`const` 隐式 static**，写 `static const` 是冗余（stdlib 零处这么写）。`const` 的完整
> 语义见 [const 编译期常量](../language/const.md)。

**私有静态魔数也走 §4**（`_camelCase`）——与私有实例字段同一形式，不引入
`_SCREAMING_SNAKE` 例外。

## 8. 命名空间：PascalCase 点分

```z42
namespace Std.IO;
namespace Std.Collections;
namespace Demo.Web.Api;
```

- 每段都是 PascalCase；
- 用 `.` 分隔（不用 `::` / `/`）；
- 公司 / 项目名通常作首段（`Acme.Web`），stdlib 用 `Std.*`；
- **不用** `_` 或 `-`（保留 `_` 给字段前缀）。

> `Std` 与 `Std.*` 是**保留命名空间**：非官方 stdlib 包声明它是硬错误。详见
> [工程文件规范](../toolchain/z42-toml.md)。

## 9. 目录：PascalCase

```
my-lib/
├── my-lib.z42.toml
├── src/
│   ├── Calculator.z42
│   ├── Collections/
│   │   ├── Dictionary.z42
│   │   └── List.z42
│   └── Net/
│       └── Http/
│           └── Client.z42
└── tests/
```

**约定**：

- `src/` 顶层目录强制叫 `src/`（工程结构，见 [工程文件规范](../toolchain/z42-toml.md)）；
- 子目录 PascalCase，可嵌套多层（`src/Net/Http/Server/`）；
- 子目录**常常**对应一段子命名空间（`src/Collections/` → `namespace Std.Collections`），
  但**不强制**——目录只是文件系统组织，不是命名空间边界。`namespace X;` 声明才是权威，
  同一目录内不同 `.z42` 文件完全可以属于不同命名空间。

**反例**：

```
src/
├── collections/             # ✗ 小写目录
├── http_server/             # ✗ snake_case
├── EmailUtils.helpers/      # ✗ `.` 在目录名里（与命名空间混淆）
```

## 10. 静态成员：与实例成员同规则

z42 的 `static` 只表达**生命周期**（与类型同生命周期，不绑定实例），**不改变命名规则**。

```z42
public class Logger {
    // 公开静态：PascalCase（同公开实例字段）
    public static Logger Default = new Logger();
    public static int MaxBufferSize = 4096;

    // 私有静态：_camelCase（同私有实例字段）
    private static int _instanceCount = 0;
}
```

**为什么不加 `s_` 前缀**：

- .NET runtime 内部代码用 `s_field` / `t_field` / `_field` 区分，但**外部公共 C# 指南不强制**；
- 现代 IDE 早已用颜色 / 字体区分 static vs instance；语义靠语言层
  （`ClassName.X` 对 `this.X`），不靠命名；
- 匈牙利式前缀（`s_` / `t_` / `m_` / `g_`）增加噪音，z42 **不引入**。

> **线程局部存储**：z42 当前没有对应语法。将来引入时**命名规则与静态字段相同**
> （公开 `PascalCase` / 私有 `_camelCase`），不加 `t_` 前缀——差异通过 attribute /
> modifier / 类型表达。

## 11. 文件名：PascalCase，匹配主类型名

```
src/
├── Calculator.z42         # public class Calculator
├── IDisposable.z42        # public interface IDisposable
├── Exceptions/
│   └── ArgumentException.z42
└── Internal/
    └── BufferPool.z42
```

**一文件一主类型**（如 C# / Java / Swift）。例外：紧密耦合的辅助类型（如 `record CacheEntry`
只服务于 `Cache`）可与主类型同文件。

> 不用 `snake_case.z42`（Rust 风格）——与命名空间 / 类型名形成视觉断层；
> 不用 `kebab-case.z42`（Web 风格）—— `-` 在标识符里非法。

## 12. 缩略词

**长度 ≤ 2 字母 → 全大写**；**≥ 3 字母 → PascalCase**：

```z42
class IOStream { }       // 2 字母：IO
class URLBuilder { }     // ✗ 错（URL = 3 字母 → Url）
class UrlBuilder { }     // ✓ 对
class XmlReader { }      // ✓ Xml
class JsonParser { }     // ✓ Json
namespace Std.IO;        // ✓
namespace Std.Net.Http;  // ✓ Http（4 字母）
```

| 缩略词 | 写法 |
|--------|------|
| IO / IP / OS | `IO` / `IP` / `OS`（2 字母全大写）|
| ID | `Id`（作为词，不视为缩略词；C# 公约也认）|
| URL / XML / JSON / HTML | `Url` / `Xml` / `Json` / `Html` |
| HTTP / HTTPS / API / UTF8 | `Http` / `Https` / `Api` / `Utf8` |

**规则源**：.NET 命名指南。Swift / Java 也认这条；Rust 因 snake_case 不存在该问题。

## 13. Boolean 标识符

布尔值用助动词或形容词前缀，表达"是不是 / 有没有 / 能不能"：

| 前缀 | 用法 | 示例 |
|------|------|------|
| `Is*` | 状态判断（瞬时属性）| `IsEmpty`, `IsNull`, `IsValid` |
| `Has*` | 拥有性判断 | `HasValue`, `HasChildren`, `HasErrors` |
| `Can*` | 能力 / 权限判断 | `CanWrite`, `CanRead`, `CanExecute` |
| `Should*` | 推荐 / 决策（少用，避免歧义）| `ShouldRetry` |
| `*ed` / `*able` 后缀 | 状态 / 性质形容词 | `Connected`, `Closed`, `Comparable` |

```z42
public bool IsEmpty;
public bool HasValue;
public bool CanWrite;

public bool Empty;       // ✗ 看起来像名词
public bool Value;       // ✗ 含义不明
```

## 14. 后缀强制（三条编译期硬规则）

这三条**不是**建议——违反直接编译失败：

| 基类 / 接口 | 类名必须以…结尾 | 诊断 |
|---|---|---|
| `: Attribute` | `Attribute` | **E0444** |
| `: Analyzer` | `Analyzer` | **E0445** |
| `: Generator` | `Generator` | **E0447** |

```z42
public class RouteAttribute : Attribute { … }   // ✓ 定义带后缀
[Route("/u")] class C { }                       // ✓ 应用时剥后缀

public class Route : Attribute { … }            // ✗ E0444
public class NoGoto : Analyzer { … }            // ✗ E0445
public class Serde : Generator { … }            // ✗ E0447
```

**语义**：

- attribute **应用时剥后缀**（类 `RouteAttribute` ↔ 用名 `[Route]`），反射引用仍用真实类名
  `typeof(RouteAttribute)`。比 C# 更严：C# 后缀可选（CA1710 是软警告）且允许双拼法
  （`[Route]` / `[RouteAttribute]`），z42 强制单拼法。
- analyzer / generator **没有用名**——handler 靠接口发现，后缀是 kind 信号。这两条 C# 无先例。

判据是**直接基类 / 直接实现的接口名**（纯语法检查，不依赖类型解析）。不派生这三者的类型
天然豁免。

## 15. Exception 类型：`Exception` 后缀（约定）

继承自 `Std.Exception` 的类型**应当**以 `Exception` 结尾：

```z42
public class TomlException : Exception { … }              // ✓
public class ProcessStartException : Exception { … }      // ✓

public class TomlError : Exception { … }                  // ✗ 用 Exception 后缀
public class TomlParseFail : Exception { … }              // ✗
```

> ⚠️ **这条不是编译期强制**——与 §14 的三条不同，编译器不检查 Exception 后缀。它是
> stdlib 与工具链遵循的约定。

- C# / Java / Python 的 `*Error` ≈ Rust 的 `*Error` ≠ z42 的 `*Exception`——z42 跟 C# 公约
  （"异常"比"错误"涵盖更广）；
- **命名空间**：通常放在所属包的根 namespace（`Std` 而非 `Std.Toml`），让 `: Exception`
  通过同命名空间查找解析到 `Std.Exception`。

## 16. Enum：单数 vs 复数

- **互斥状态枚举** → **单数**类型名；
- **位标志枚举**（bit-OR 组合）→ **复数**类型名（.NET 公约）。

```z42
// ✓ 互斥单数
public enum Direction { North, South, East, West }
public enum LogLevel  { Debug, Info, Warning, Error }
public enum TokenKind { Identifier, Number, String, Operator }

// ✓ 位标志复数（每个成员仍是 PascalCase 单数）
public enum FileAccess  { Read = 1, Write = 2, ReadWrite = 3 }
public enum WatchEvents { Created = 1, Modified = 2, Deleted = 4 }
```

**成员命名**：永远 PascalCase，**不带类型前缀**（不要 `LogLevel.LL_INFO`、
不要 `Direction.DirNorth`）。

> z42 当前**没有** `[Flags]` attribute，也不支持在 enum 上做 `|` 位运算。stdlib 里需要
> 位标志的地方（如 `Std.ModeFlags`）用"类 + `static int` 常量 + 手工位运算"代替。
> 这类容器同样按位标志规则取复数名。

## 17. 成员名与类型名同名（stuttering）

```z42
public class Shape {
    public Point Point;     // ✗ 字段 "Point" 类型也是 Point —— 阅读时双倍负担
    public Color Color;     // ✗ 同样
    public Type Type;       // ✗✗ 还和保留语义撞车
}
```

`shape.Point.X` 读起来要在脑子里区分"第一个 Point 是字段名"和"潜在的 Point 类"，是无意义的
认知开销。.NET FxCop CA1721 与 Go 的 Effective Go 都明确反对。

**处理建议（按优先级）**：

1. **用角色 / 用途命名**：`Point Position` / `Point Origin` / `Color Background` / `Color Stroke`
2. **基数 / 关系前缀**：`Node Source` / `Node Target`、`DateTime Start` / `DateTime End`
3. **用 `Kind` / `Category` 替代 `Type`**：`TokenKind Kind`（`Type` 与
   `Std.Reflection.Type` 撞名，用户字段尽量避开）
4. **域语义**：`User Customer` / `User AssignedAgent`、`User Author` / `User Assignee`
5. **集合用复数**：`List<User> Users`
6. **仅在唯一识别属性时允许同名**：`class Brush { public Color Color; }`——很少真合理，
   提交前问自己"是否真的找不到比类型名更具体的角色名？" 99% 找得到。

**反例集中重申**：

```z42
// ✗ stuttering              // ✓ 改名表达"它在结构里的角色"
public Point Point;          public Point Position;       // 或 Origin / Center / Anchor
public Color Color;          public Color Background;     // 或 Foreground / Stroke / Fill
public Status Status;        public Status State;         // 或 CurrentStatus
public Type Type;            public TokenKind Kind;
public User User;            public User Owner;           // 或 Author / Assignee / Customer
public Address Address;      public Address ShippingAddress;
```

> 经验法则：**字段名应该回答"它在结构里扮演什么"（角色），不应该重复"它是什么类型"。**

## 18. 方法动词约定

| 前缀 / 模式 | 用途 | 例 |
|------|------|------|
| `Get*` / `Set*` | 单值访问器（Get 无副作用，Set 幂等）| `config.SetTimeout(ms)` |
| `Find*` | 查找，**找不到返回 null / -1 / 可空值** | `list.Find(predicate)` |
| `Search*` | 多次匹配查询，**返回集合 / 迭代器** | `tree.Search(query)` |
| `Add*` / `Remove*` / `Insert*` / `Clear*` | 集合修改 | `list.Add(x)`, `dict.Remove(key)` |
| `Open*` / `Close*` / `Dispose*` | 资源生命周期 | `stream.Close()`, `token.Dispose()` |
| `Read*` / `Write*` | IO 操作 | `File.ReadAllText(p)`, `File.WriteAllBytes(p, b)` |
| `Try*` | 失败不抛异常（见下）| `int.TryParse(s)`, `dict.TryAdd(k, v)` |
| `Parse*` | 字符串 → 值，**失败抛异常**（与 `TryParse*` 成对）| `int.Parse(s)` |
| `ToX` / `AsX` | 类型转换：`ToX` 复制 / 转换；`AsX` 视图 / 断言 | `list.ToArray()` |
| `Is*` / `Has*` / `Can*` | bool 谓词（见 §13）| `IsValid()`, `CanWrite()` |
| `Compare*` / `Equals` | 比较 | `a.CompareTo(b)`, `a.Equals(b)` |
| `Copy*` / `Clone*` | 复制：`Copy*` 写入目标；`Clone` 返回新实例 | `Array.Copy(src, dst, n)` |

**`Try*` 的返回形状**：z42 的 `Try*` 有两种形态，**都不用 `out` 参数**：

```z42
public static int Parse(string s)        // 失败抛 FormatException
public static int? TryParse(string s)    // 失败返回 null（可空返回，不是 out 参数）

public bool TryAdd(TKey key, TValue v)   // 只需"成功与否"时返回 bool
```

`Parse` / `TryParse` 应当**同时存在**，让调用方选择 throw 还是 check 风格。

> 语言里有 `out` 关键字，但 stdlib **零处**用 `out` 做 `Try*` 的出参——统一走可空返回。
> 新 API 请跟随这个形状。

## 19. 反"否定式"命名

```z42
// ✗ 否定式作为名字          // ✓ 正向 + 调用方取反
public bool IsNotEmpty;       public bool IsEmpty;          // 调用方写 !IsEmpty
public bool DisableLogging;   public bool IsLoggingEnabled;
public void WithoutCache();   public void DisableCache();
public bool IsDisabled;       public bool IsEnabled;
```

否定式叠加（`!IsNotEmpty`）需要读者绕两道脑筋。**默认值能不能映射到 `false`** 也是判断点：
`IsEnabled = true`（默认 enable）比 `IsDisabled = false`（双重否定）更自然。

**唯一可接受的否定式**：当**正向用语在领域里就是少见 / 不自然**时：

```z42
public bool IsReadOnly;     // ✓ "ReadOnly" 是单一概念，比 "IsWritable = false" 更直接
public bool IsAbstract;     // ✓ 同上
public bool IsSealed;       // ✓ 同上
```

判断准则：**领域里大家都说 "read-only" 而不是 "non-writable"** → 用 `IsReadOnly`；
否则取正向。

## 20. 构造器 / 工厂方法

- **首选 `new` 构造器**：直接、明确
  ```z42
  var box = new Box<int>(42);
  ```
- **`static Create(…)` 工厂**：需要多步初始化、参数推断，或返回**子类型** / **缓存实例**
  ```z42
  var p = Process.Create(argv).WithEnv("KEY", "value").Spawn();
  ```
- **`static From<X>(X x)` 转换工厂**：从其他类型构造
  ```z42
  var dt    = DateTime.FromUnixMs(1700000000000);
  var color = Color.FromHex("#ff8800");
  ```
- **`static Empty()` / `Default`**：空 / 默认实例
- **不允许** `Make*` / `Construct*` / `New*` 作通用工厂前缀。`Build*` 仅用于真正的 fluent
  builder（`new StringBuilder().Append(…).Build()`），且 `Build()` 返回的应是**不同类型**
  （builder → result）。

## 21. Lambda 与 discard `_`

**Lambda 参数命名**：

- 单参短名 OK：`list.FindAll(x => x > 0)`；
- 多参语义化：`(key, value) => …`（不是 `(a, b)`）；
- 嵌套 lambda 禁止同名遮蔽：

```z42
// ✗ 内外同名遮蔽
Walk(node, item => Walk(item, item => Use(item.Id)));
// ✓ 各层用各层的角色名
Walk(node, parent => Walk(parent, child => Use(child.Id)));
```

**Discard `_`**：

```z42
(_, value) = pair;                     // 解构，只要 value
if (p is (0, var label)) { … }         // 元组模式里的位置通配
switch (p) { case (_, 0): … }
```

- `_` 不是标识符，**不能**之后引用（它是模式匹配 / 解构的通配符）；
- **不允许** `_` 当字段 / 局部名——私有字段必须 `_x`（至少 2 字符）。

> 元组解构写作 `(a, b) = e`，**前面不加 `var`**；细节见
> [元组与元组模式](../language/tuples.md)。

## 22. Async 后缀：当前不约束

C# 公约是 `async Task<T> LoadAsync()`（`Async` 后缀）。z42 的 async/await 尚未落地，
后缀策略留待彼时决定。**目前不要**主动给方法加 `Async` 后缀。

---

## 反模式速查

| 反模式 | 为什么不要 | 改用 |
|--------|----------|------|
| `class _Foo` | `_` 前缀仅用于私有字段 | `internal class Foo` |
| `private int Count` | 私有应带 `_` | `private int _count` |
| `int X, Y, Z` 字段全单字母 | 仅 3D 数学 / 向量场景可 | `public int Width, Height` |
| `bool valid` | 不带前缀（看似名词）| `bool isValid` |
| `interface FooInterface` | 用 `I` 前缀，别用后缀 | `interface IFoo` |
| `interface IIFooMonad` | 双 `I` 是 typo / 噪音 | `interface IFooMonad` |
| `var ll = items.Count()` | 缩写 / 单字母变量 | `var itemCount = …` |
| `class TYPE_REGISTRY` | 类不用 SCREAMING_SNAKE | `class TypeRegistry` |
| `Map<K, V>` 单字母泛型参数 | z42 用 `TKey` / `TValue` | `Map<TKey, TValue>` |
| `const int kMaxSize` | C++ 风格 `k` 前缀过时 | `const int MaxSize` |
| `static const int X` | `const` 已隐式 static | `const int X` |
| `Util` / `Helper` / `Manager` / `Service` 后缀 | 含糊；改用动作或域名 | `StringFormatter`, `OrderProcessor` |
| 中英混合标识符 / 拼音 | 工具链 / IDE 兼容性差 | 英文 |
| `bool IsNotEmpty` | 双重否定 `!IsNotEmpty` 难读 | `bool IsEmpty`，调用方取反 |
| `class TomlError : Exception` | Exception 类型缺后缀 | `class TomlException : Exception` |
| `class Route : Attribute` | **编译错误 E0444** | `class RouteAttribute : Attribute` |
| `enum DayOfWeeks` | 互斥状态用复数 | `enum DayOfWeek`（单数）|
| `LogLevel.LL_DEBUG` | enum 成员加类型前缀 | `LogLevel.Debug` |
| `MakeFoo()` / `NewFoo()` 工厂 | 用 `Create*` / `From*` | `static Foo Create(…)` |
| `static Foo Build()` 返回 Foo | `Build()` 仅给真正的 builder | `static Foo Create(…)` |
| Lambda 嵌套同名 `x => f(x, x => …)` | 遮蔽难读 | `outer => f(outer, inner => …)` |
| 私有字段 `private int _` | 单字符 `_` 与 discard 冲突 | `private int _count` |

---

## 与其他语言对照

| 维度 | z42 | C# | Rust | Java | Swift | Go | Python (PEP 8) |
|------|------|------|------|------|------|------|--------|
| 类型 | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` |
| 方法 | `PascalCase` | `PascalCase` | `snake_case` | `camelCase` | `camelCase` | `PascalCase`（导出）| `snake_case` |
| 字段（公开）| `PascalCase` | `PascalCase`（属性）| `snake_case` | `camelCase` | `camelCase` | `PascalCase` | `snake_case` |
| 字段（私有）| `_camelCase` | `_camelCase` | `snake_case` | `camelCase` | `camelCase` | `camelCase` | `_snake_case` |
| 局部 / 参数 | `camelCase` | `camelCase` | `snake_case` | `camelCase` | `camelCase` | `camelCase` | `snake_case` |
| 常量 | `PascalCase` | `PascalCase` | `SCREAMING_SNAKE` | `SCREAMING_SNAKE` | `camelCase` | `PascalCase` | `SCREAMING_SNAKE` |
| 泛型参数 | `T` / `TKey` | `T` / `TKey` | `T` / `K` | `T` / `K` | `T` / `Key` | — | `T` |
| 接口前缀 | `I*` | `I*` | 无（trait）| 无 | 无（protocol）| 无（`-er` 后缀）| 无 |
| 缩略词 | `Url` / `IO` | `Url` / `IO` | 全 snake | `URL`（旧）/ `Url`（新）| 同 Java | `URL` | `URL` |
| 文件名 | `Calculator.z42` | `Calculator.cs` | `calculator.rs` | `Calculator.java` | `Calculator.swift` | `calculator.go` | `calculator.py` |

**z42 立场总结**：

- **形状**借 C# / Swift（PascalCase 类型 / camelCase 值的二元体系）；
- **接口前缀 `I`** 跟 C# 而非 Swift；
- **私有字段 `_` 前缀**跟 C# 主流（区分参数 / 局部）；
- **常量 PascalCase** 跟 C# 而非 Rust；
- **缩略词**跟 .NET 公约（2 字母全大写，3+ PascalCase）；
- **不用** Go 的大小写编码可见性，**不用** Rust 的 snake_case 函数。

---

## 何时偏离

除 §14 的三条编译期硬规则外，本文的规则**不强制**（不会 lint fail）。允许偏离的场景：

1. **第三方 ABI 兼容**：调用 C / native 库时类型名要直接对应外部命名
   （`extern struct GLFWwindow`）—— `extern` / native 边界尊重外部命名；
2. **数学符号**：`Vector3.X/Y/Z`、`Matrix4.M00/M01`、`Complex.Re/Im` 等约定俗成的单字母字段；
3. **DSL / 生成代码**：SQL 映射、protobuf 生成等可保留源命名；
4. **历史代码迁移**：从其他语言移植时可暂保留原命名，迁移完成后批量重命名。

## 关联文档

- [const 编译期常量](../language/const.md) —— `const` 的完整语义（隐式 static、无存储）
- [访问权限控制](../language/access-control.md) —— `public` / `private` / `internal` 的语义
- [委托与事件](../language/delegates-events.md) —— `Action` / `Func` / `MulticastX` 的命名由来
- [工程文件规范](../toolchain/z42-toml.md#包名命名规则) —— 包名（`[project] name`）规则
- [错误码](../appendix/error-codes.md) —— 诊断码总表
