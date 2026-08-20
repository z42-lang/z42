# 命名规范

> z42 用户代码命名约定。本文是**强建议**（用户违反不会编译失败，但 stdlib / 工具链 / `z42-fmt` 默认风格按此为准），不是语法规则。
>
> 设计目标：让代码"看上去就知道是什么"——类型 vs 值、公开 vs 私有、字段 vs 局部，**零歧义零思考**。规范从 C# / Rust / Swift / Go / Python (PEP 8) 各取所长。

---

## 速查表

| 标识符类型 | 约定 | 示例 |
|------|------|------|
| **类 / 结构体 / record / enum** | `PascalCase` | `class Calculator`, `enum Direction`, `record Point` |
| **Primitive 类型 struct（stdlib）** | BCL `PascalCase`（`Int32 / Boolean / SByte / ...`）；keyword（`int / bool / i8 / ...`）是 source-level alias，参 C# `int` ⟷ `System.Int32` | `public struct Int32 : ...`, `public struct Boolean : ...` |
| **接口** | `I` + `PascalCase` | `interface IDisposable`, `IEnumerable<T>` |
| **委托 / 事件类型** | `PascalCase`（Action / Func 例外）| `delegate void OnClick(...)`, `Action<int>` |
| **方法 / 函数** | `PascalCase` | `void Main()`, `int Add(int a, int b)` |
| **属性 / 公开字段** | `PascalCase` | `public int Count`, `public string Name` |
| **私有字段** | `_` + `camelCase` | `private int _count`, `private string _name` |
| **公开静态字段 / 方法** | `PascalCase`（同实例公开成员）| `public static int MaxRetries`, `Math.Sqrt(x)` |
| **私有静态字段** | `_` + `camelCase`（同实例私有）| `private static int _instanceCount` |
| **线程局部变量**（L3 引入后）| 与静态字段同 | (待 L3 落地后定型；见 Deferred) |
| **局部变量 / 参数** | `camelCase` | `var index = 0`, `void f(int leftValue)` |
| **常量 / 静态只读** | `PascalCase` | `const double Pi = 3.14`, `static readonly DateTime Epoch` |
| **泛型类型参数** | 单字母 `T` 或 `TPascalCase` | `class Box<T>`, `Dictionary<TKey, TValue>` |
| **命名空间** | `PascalCase` 点分 | `Std.IO`, `Demo.Web.Api` |
| **包名（manifest project name）**| 小写点分 | `z42.core`, `z42.io`, `acme.web` |
| **目录名** | `PascalCase`，对应 namespace 段 | `src/Collections/`, `src/Exceptions/` |
| **文件名** | 与主类型同名 `PascalCase.z42` | `Calculator.z42`, `IDisposable.z42` |
| **Bool 方法 / 字段** | `Is*` / `Has*` / `Can*` / `*ed` / `*able` 前缀 | `IsEmpty`, `HasValue`, `CanWrite`, `IsParsed` |
| **方法动词** | `Get/Set/Find/Add/Remove/Open/Close/Try/Parse/To/As/...` | `dict.TryGet(key)`, `int.Parse(s)`, `list.ToArray()` |
| **Exception 类型** | `*Exception` 后缀强制 | `TomlException`, `ArgumentException`, `ProcessStartException` |
| **Enum** | 普通单数 / `[Flags]` 复数；成员 PascalCase 不带前缀 | `enum Direction { North, ... }`, `[Flags] enum FileAccess { Read, ... }` |
| **工厂方法** | `static Create*` / `From*` / `Empty()` / `Default` | `Color.FromHex("...")`, `Dictionary.Empty()` |
| **Lambda discard** | `_` 表忽略（不能后续引用）| `pairs.Map((_, v) => v)` |

---

## 设计原则

### 1. 视觉层级：类型 vs 值

**类型大写开头，值小写开头**。一眼看出哪是类型哪是值，无需 IDE 高亮：

```z42
var greeter = new Greeter();   // Greeter 大写 = 类型；greeter 小写 = 实例
int count = list.Count();      // int = 内置类型；count = 值；Count = 方法
```

借鉴 Rust（`fn foo() -> Foo` 中 `foo` 和 `Foo` 视觉区分），同时保留 C#-style 的 `PascalCase` 用于"东西的名字"。

**Primitive 类型例外（keyword ⟷ BCL struct alias）**：源代码层有两种等价拼写：

```z42
int x = 5;          // keyword 形式（推荐 — 日常代码用）
Int32 y = 5;        // BCL form（等价 — 同义于 C# `int` ⟷ `System.Int32`）

x.ToString();       // dispatch 到 Std.Int32.ToString
Int32.Parse("42");  // 等价 int.Parse("42")
```

stdlib `struct` 名采用 BCL PascalCase（`Boolean / Char / SByte / Int16 / Int32 / Int64 /
Byte / UInt16 / UInt32 / UInt64 / Single / Double`），文件名 = struct 名（`Int32.z42` /
`Boolean.z42`）。z42 关键字（`int / long / bool / char / float / double / i8 / i16 / u8 /
u16 / u32 / u64`）是 source-level alias，由 TypeChecker 通过 `TypeRegistry.StdlibClassName`
映射到 stdlib FQN（`Std.Int32` 等）。

| keyword | struct | 文件 |
|---------|:------:|:----:|
| `int` | `Int32` | `Primitives/Int32.z42` |
| `long` | `Int64` | `Primitives/Int64.z42` |
| `i8` | `SByte` | `Primitives/SByte.z42` |
| `bool` | `Boolean` | `Primitives/Boolean.z42` |
| `float` | `Single` | `Primitives/Single.z42` |
| `double` | `Double` | `Primitives/Double.z42`（拼写一致） |
| `char` | `Char` | `Primitives/Char.z42`（拼写一致） |
| ...     | ...    | （完整 12 条见 [rename-primitives-to-pascal-case spec](../../spec/archive/2026-05-24-rename-primitives-to-pascal-case/proposal.md#映射)） |

历史背景见 [rename-primitives-to-pascal-case](../../spec/archive/2026-05-24-rename-primitives-to-pascal-case/) (2026-05-24)。

### 2. 可见性靠 `public` / `private` 关键字，不靠命名

Go 用大小写编码可见性（`Foo` exported / `foo` unexported）有简洁性优势，但代价是限制了**值标识符的命名自由**——比如 Go 必须 `func main()` 小写。

z42 走 C# / Java / Kotlin 路线：**可见性通过关键字 (`public` / `private` / `protected` / `internal`) 显式声明**，命名约定与可见性正交。这样：

- 公开方法 `public int Sum()` 与私有方法 `private int Sum()` 都用 `PascalCase`
- 公开字段 `public int Count` 与私有字段 `private int _count` 形状一致但 `_` 区分（见 §4）

### 3. `I` 前缀保留给接口

C# 的 `I` 前缀（`IDisposable`, `IEnumerable`）**保留**。Swift / Kotlin / Rust trait 不带前缀（`Comparable`, `Hashable`, `Iterator`），但 z42 选 C# 是因为：

- z42 stdlib 已建立此约定（`IDisposable`, `IComparable<T>`, `IEnumerable<T>`, `INumber<T>` 等）
- 接口与类在使用点（`new T()` 不能用于接口）的语法差异不显然，类名前 `I` 提供即时识别
- 工具如 `z42c info` 列符号时 `I` 前缀让接口排在一起

> **不要用 `T` 前缀给 trait**（Rust 的 `Trait` 后缀不流行，TypeScript 的 `TFoo` 也不流行）—— `I` 前缀是 C# 公约的明确选择。

### 4. 字段：公开 PascalCase，私有 `_camelCase`

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

- 与局部变量 / 参数（同样 camelCase）区分。在方法体内 `this._refCount` 与 `refCount`（参数）一眼可辨
- C# 主流公约；z42 stdlib 已落地（TomlParser / TomlWriter / 等都用 `_field`）
- 比 `m_` 或 `s_`（Hungarian 退化）更简洁

> **不允许** `_` 前缀用于公开字段或方法 —— 下划线前缀 ≡ "私有/内部"。

### 5. 局部变量 / 参数：camelCase

```z42
void ProcessOrder(int orderId, string customerName) {   // 参数 camelCase
    var totalAmount = 0;          // 局部 var camelCase
    var itemCount = 0;
    foreach (var item in items) { // 'item' 简短局部
        totalAmount += item.Price;
        itemCount++;
    }
}
```

参数和局部变量同形状，因为它们的**生命周期一致**（栈作用域）。无下划线，无 PascalCase。

### 6. 泛型类型参数

借鉴 C# 公约：

- **单参数用 `T`**：`class List<T>`, `T Identity<T>(T x)`
- **多参数用 `T` + 语义名**：`Dictionary<TKey, TValue>`, `Func<TInput, TOutput>`
- **不要用单字母 K / V / E / R**（Rust / Java 风格）—— z42 用 `TKey` / `TValue` / `TElement` / `TResult`

**理由**：单字母 `T` 易识别为"类型参数"（与具体类型 `T` 区分靠语境），但多个并列单字母（`Map<K, V>`）易混淆。语义前缀 `T` 让"这是类型参数"显式。

```z42
class Cache<TKey, TValue> { ... }
TResult Map<TInput, TResult>(TInput x, Func<TInput, TResult> f) { ... }
```

### 7. 常量 / 静态只读：PascalCase

```z42
class Math {
    public static const double Pi      = 3.14159265;
    public static const double E       = 2.71828183;
    public static const int MaxRetries = 3;
}
```

**不使用 `SCREAMING_SNAKE_CASE`**：

- Rust / Java / Python 用 `MAX_VALUE` 风格强调"编译时常量"
- 但 z42 跟随 C# / Swift 公约：常量是"值"，但**命名层级与类型对齐为 PascalCase**，因为：
  - 与同模块的 PascalCase 方法、类型保持单一视觉风格
  - `Math.Pi` 比 `Math.PI` 在 dotted 访问时更自然
  - 区分 mutable 值靠 `const` 关键字，不靠 case

**私有静态魔数也走 §4**（`_camelCase`）—— 与私有实例字段同一形式，不引入 `_SCREAMING_SNAKE` 例外（fix-stdlib-naming-violations, 2026-05-24 删除该例外条款；§4 + §7 唯一规则消除冗余）。

### 8. 命名空间：PascalCase 点分

```z42
namespace Std.IO;
namespace Std.Collections.Generic;
namespace Demo.Web.Api;
```

- 每段都是 PascalCase
- 用 `.` 分隔（不用 `::` / `/`）
- 公司 / 项目 名通常作首段（`Acme.Web`），stdlib 用 `Std.*`
- **不用** `_` 或 `-`（与文件名 `Foo-Bar.z42` 不一致；保留 `_` 给字段前缀）

> 与 C# / Java 公约对齐；Rust 用 `crate::module` 是因为 Cargo 拥有 crate root 概念，z42 用 `.` 点分扁平化更接近 Python / .NET 阅读体验。

### 9. 包名（manifest `name`）：小写点分

包是分发单位（一个 `z42.toml` `[project]` = 一个包），与命名空间是**两个独立概念**：

```toml
# z42.toml
[project]
name    = "z42.collections"      # ✓ 小写，点分
version = "0.1.0"

[dependencies]
"z42.core" = "0.1.0"             # ✓ 引用时也小写
"acme.web" = "1.2.0"             # ✓ 第三方包
```

- 段间用 `.`，全小写
- 通常按"组织.功能" 形式（`z42.io`, `acme.payments`, `unity.physics`）
- 与命名空间映射约定：包 `z42.collections` 内含 `namespace Std.Collections`、`namespace Std.Collections.Generic` 等（包名指代"谁拥有"，命名空间指代"代码住哪儿"）
- **不允许** `_` / `-` —— `.` 是唯一分隔符
- **stdlib 命名**：`z42.<topic>`（`z42.core`, `z42.io`, `z42.math`, `z42.test`, ...）

**为什么包名小写、命名空间 PascalCase**？

| 维度 | 包名 | 命名空间 |
|------|------|---------|
| 出现位置 | `z42.toml` 配置、CLI（`z42c build z42.io`）、文件系统目录 | 源代码 `namespace`/`using` 语句 |
| 作用 | 分发 / 依赖管理（构建系统的"地址"） | 类型查找 / 符号路径（编译期的"路径"） |
| 用户输入习惯 | 命令行 / TOML：小写更易输入 | 代码内：跟类型名同风格便于阅读 |
| 对照 | npm `@org/lib`、Cargo `tokio`、Maven `com.acme:lib` 全小写 | C# / Java namespace PascalCase |

z42 采纳：包名按发布层世界（npm / Cargo / pip 都小写）公约，命名空间按代码层世界（C# / Java）公约。

> 同名变体冲突时（如 `z42.IO` vs `z42.io`），编译器视为冲突报错。

### 10. 目录：PascalCase，对齐命名空间

```
src/libraries/z42.core/
├── z42.core.z42.toml         # 包 manifest（小写）
├── src/
│   ├── Array.z42             # public class Array (namespace Std)
│   ├── Object.z42
│   ├── Collections/          # → namespace Std.Collections
│   │   ├── Dictionary.z42
│   │   ├── KeyValuePair.z42
│   │   └── List.z42
│   ├── Exceptions/           # → namespace Std.Exceptions
│   │   ├── Exception.z42
│   │   └── ArgumentException.z42
│   └── Delegates/            # → namespace Std.Delegates
│       └── ISubscription.z42
```

**约定**：

- `src/` 顶层目录强制叫 `src/`（项目结构 — 详见 [project.md](../compiler/project.md)）
- 子目录 PascalCase，**通常**对应一段子命名空间（`src/Collections/` → `namespace Std.Collections`），但不强制——只是用户友好的导航
- 子目录可嵌套多层（`src/Net/Http/Server/`）
- 同一目录内**所有 `.z42` 文件可属不同命名空间**——目录只是文件系统组织，不是命名空间边界。`namespace X;` 声明才是权威
- **`tests/`**（与 `src/` 平级）放 lib 自测，文件 snake_case 可选（`tests/string_format.z42`）—— 自测不公开

**反例**：

```
src/
├── collections/             # ✗ 小写目录
├── http_server/             # ✗ snake_case
├── EmailUtils.helpers/      # ✗ `.` 在目录名里（与命名空间混淆）
```

### 11. 静态变量 / 线程局部变量：与实例字段同规则

z42 的 `static` 关键字只表达**生命周期**（与类型同生命周期，不绑定实例），**不改变命名规则**。

```z42
public class Logger {
    // 公开静态：PascalCase（同公开实例字段）
    public static Logger Default = new Logger();
    public static int MaxBufferSize = 4096;

    // 私有静态：_camelCase（同私有实例字段）
    private static int _instanceCount = 0;
    private static Dictionary<string, Logger> _registry = new Dictionary<string, Logger>();
}
```

**为什么不加 `s_` 前缀**？

- Microsoft 内部 .NET runtime 代码用 `s_field` (static) / `t_field` (thread-static) / `_field` (instance)——但**外部公共 C# 指南不强制**
- 现代 IDE 早已用颜色 / 字体区分 static vs instance；语义靠语言层（`ClassName.X` 对 `this.X`），不靠命名
- Hungarian-y 前缀（`s_` / `t_` / `m_` / `g_`）增加噪音，z42 决定**不引入**

**线程局部变量**（L3 concurrency 落地后）：

z42 当前**没有**线程局部存储语法。当 0.8.x async/await + 多线程引入时，预期表达形式（候选）：

```z42
// 候选 1：attribute
[ThreadLocal]
private static int _localCounter;

// 候选 2：modifier
private threadlocal int _localCounter;

// 候选 3：泛型容器
private static ThreadLocal<int> _localCounter = new ThreadLocal<int>();
```

**确定的命名约定**：无论最终语法选 1/2/3，**命名规则与静态字段相同**（`PascalCase` 公开 / `_camelCase` 私有），不加 `t_` 前缀。差异通过 attribute / modifier / 类型表达。

> 0.8.x concurrency 落地时正式更新此节（见 Deferred §naming-conv-5）。

### 9. 文件名：PascalCase，匹配主类型名

```
src/
├── Calculator.z42         # public class Calculator
├── IDisposable.z42        # public interface IDisposable
├── Exceptions/
│   └── ArgumentException.z42
└── Internal/
    └── BufferPool.z42
```

**一文件一主类型**（如 C# / Java / Swift）。例外：紧密耦合的 1+N 私有辅助类型（如 `record CacheEntry` 内嵌在 `Cache.z42` 中）可同文件。

> 不用 `snake_case.z42`（Rust 风格）—— 与命名空间 / 类型名形成视觉断层；不用 `kebab-case.z42`（Web 风格）—— `-` 在标识符里非法。

### 10. 缩略词

**长度 ≤ 2 字母 → 全大写**；**≥ 3 字母 → PascalCase**：

```z42
class IOStream { }       // 2 字母：IO
class URLBuilder { }     // ✗ 错（URL=3字母 → Url）
class UrlBuilder { }     // ✓ 对
class XmlReader { }      // ✓ Xml（3 字母）
class JsonParser { }     // ✓ Json
class HtmlEncoder { }    // ✓ Html
namespace Std.IO;        // ✓
namespace Std.Net.Http;  // ✓ Http（4 字母）
```

**规则源**：.NET 命名指南（"capitalize the first letter only; if acronym is 2 letters, capitalize both"）。这条规则 Swift / Java 也认；Rust 因 snake_case 不存在该问题（`parse_url`）。

| 缩略词 | 写法 |
|--------|------|
| IO     | `IO` (2字母全大写) |
| IP     | `IP` |
| OS     | `OS` |
| ID     | `Id`（特殊，作为词不视为缩略词；C# 公约也认）|
| URL    | `Url` (3字母 PascalCase) |
| XML    | `Xml` |
| JSON   | `Json` |
| HTML   | `Html` |
| HTTP   | `Http` |
| HTTPS  | `Https` |
| API    | `Api` |
| UTF8   | `Utf8` |

### 11. Boolean 标识符

布尔值用助动词或形容词前缀，表达"是不是 / 有没有 / 能不能"：

| 前缀 | 用法 | 示例 |
|------|------|------|
| `Is*`   | 状态判断（瞬时属性）| `IsEmpty`, `IsNull`, `IsValid` |
| `Has*`  | 拥有性判断 | `HasValue`, `HasChildren`, `HasErrors` |
| `Can*`  | 能力 / 权限判断 | `CanWrite`, `CanRead`, `CanExecute` |
| `Should*` | 推荐 / 决策（少用，避免歧义）| `ShouldRetry` |
| `*ed` / `*able` 后缀 | 状态 / 性质形容词 | `Connected`, `Closed`, `Comparable`, `Iterable` |

```z42
public bool IsEmpty;
public bool HasValue;
public bool CanWrite;
public bool IsConnected;     // 也可以
```

**反例**：

```z42
public bool Empty;       // ✗ 看起来像名词
public bool Value;       // ✗ 含义不明
public bool _writable;   // 私有 ok 但漏前缀
```

### 12. Async（L3 待引入；当前不约束）

C# 公约：`async Task<T> LoadAsync()`（`Async` 后缀）。

z42 L3 async/await 引入时再决策。**目前禁止**用户代码主动添加 `Async` 后缀，避免命名空间污染——等语言层提供 async/await 后统一选择。

---

### 12. 成员名与类型名同名（stuttering）

经典纠结点：

```z42
public class Point { public int X; public int Y; }

public class Shape {
    public Point Point;     // ✗ 字段 "Point" 类型也是 Point —— 阅读时双倍负担
    public Color Color;     // ✗ 同样
    public Type Type;       // ✗✗ 还和保留语义撞车
}
```

`shape.Point.X` 读起来要在脑子里区分"第一个 Point 是字段名"和"潜在的 Point 类"，是无意义认知开销。这是 .NET FxCop CA1721 / Go Effective Go "the name of the variable inside the method should refer to its role" 都明确反对的。

**处理建议（按优先级）**：

1. **用角色 / 用途命名，不要用类型名**

   ```z42
   public class Shape {
       public Point Position;       // ✓ Position 表达"在哪里"
       public Point Origin;         // ✓ Origin 是"基准点"
       public Color Background;     // ✓ Background 是"哪个 Color"
       public Color Stroke;
   }
   ```

2. **基数 / 关系前缀**：源 / 目标 / 起 / 终 / 父 / 子

   ```z42
   public class Edge {
       public Node Source;          // ✓ 不是 "Node Node"
       public Node Target;
   }
   public class Range {
       public DateTime Start;       // ✓ 不是 "DateTime DateTime"
       public DateTime End;
   }
   ```

3. **用 `Kind` / `Category` 替代 `Type`**

   ```z42
   public class Token {
       public TokenKind Kind;       // ✓ 而不是 TokenType Type
   }
   public class Animal {
       public AnimalCategory Category;
   }
   ```

   `Type` 在 z42 是潜在的反射 / metadata 概念名（与 `Std.Reflection.Type` 撞名），用户字段尽量避开 `Type` 这个名字。

4. **域语义**

   ```z42
   public class Order {
       public User Customer;        // ✓ 不是 "User User"
       public User AssignedAgent;
   }
   public class Issue {
       public User Author;
       public User Assignee;
   }
   ```

5. **集合用复数**

   ```z42
   public List<User> Users;         // ✓ "Users" 是 List 的角色（多个），不与 User 类撞
   ```

6. **仅在 singleton / 标识情况允许同名**

   ```z42
   public class ColorPalette {
       public Color Primary;
       public Color Secondary;
   }
   public class Brush {
       public Color Color;          // ⚠ 仅当 Brush 就是"被某 Color 染色" —— Brush.Color 是该 Brush 的唯一识别属性
   }
   ```

   这种用法**很少**真合理。提交前问自己："是否真的找不到比类型名更具体的角色名？" 99% 找得到。

**反例集中重申**：

```z42
// ✗ stuttering
public Point Point;
public Color Color;
public Status Status;
public Type Type;
public User User;
public Address Address;

// ✓ 改名表达"它在结构里的角色"
public Point Position;       // 或 Origin / Center / Anchor
public Color Background;     // 或 Foreground / Stroke / Fill
public Status State;         // 或 CurrentStatus
public TokenKind Kind;       // 而不是 Type Type
public User Owner;           // 或 Author / Assignee / Customer
public Address ShippingAddress;  // 或 BillingAddress
```

> 经验法则：**字段名应该回答"它在结构里扮演什么"（角色），不应该重复"它是什么类型"（类型）**。

### 13. 方法动词约定

标准动作前缀，让 API surface 跨 stdlib / 用户代码一致：

| 前缀 / 模式 | 用途 | 例 |
|------|------|------|
| `Get*` / `Set*` | 单值访问器（无副作用 Get，幂等 Set）| `dict.Get(key)`, `config.SetTimeout(ms)` |
| `Find*` | 查找，**找不到返回 null / -1 / `Optional<T>`** | `list.Find(predicate)`, `str.FindFirst(c)` |
| `Search*` | 多次匹配查询，**返回集合 / iterator** | `tree.Search(query)` |
| `Add*` / `Remove*` / `Insert*` / `Clear*` | 集合修改 | `list.Add(x)`, `dict.Remove(key)` |
| `Open*` / `Close*` / `Dispose*` | 资源生命周期 | `File.Open(path)`, `stream.Close()` |
| `Read*` / `Write*` | IO 操作 | `reader.ReadLine()`, `writer.Write(bytes)` |
| `Try*` | 失败不抛异常，**返回 bool**（成功时 out 参数 / Optional）| `int.TryParse(s, out n)`, `dict.TryGet(key)` |
| `Parse*` | 字符串 → 值，**失败抛异常**（与 `TryParse*` 成对）| `int.Parse(s)`, `Url.Parse(s)` |
| `ToX` / `AsX` | 类型转换：`ToX` 复制 / 转换；`AsX` 视图 / 类型断言 | `list.ToArray()`, `obj.As<IFoo>()` |
| `Is*` / `Has*` / `Can*` | bool 谓词（见 §11）| `IsValid()`, `HasChildren()`, `CanWrite()` |
| `Compare*` / `Equals` | 比较 | `a.CompareTo(b)`, `a.Equals(b)` |
| `Copy*` / `Clone*` | 复制：`Copy*` 写入目标；`Clone` 返回新实例 | `Array.Copy(src, dst)`, `obj.Clone()` |

**Try/Parse 配对约定**：

```z42
public static int Parse(string s)                   // 失败抛 FormatException
public static bool TryParse(string s, out int n)    // 失败返回 false，不抛
```

两者必须**同时存在**（提供 user 选择是 throw 还是 check 风格）。

### 14. Exception 类型：`Exception` 后缀强制

所有继承自 `Std.Exception` 的类型**必须**以 `Exception` 结尾：

```z42
public class TomlException : Exception { ... }              // ✓ stdlib 既有
public class ProcessStartException : Exception { ... }      // ✓
public class InvalidMarshalException : Exception { ... }    // ✓

public class TomlError : Exception { ... }                  // ✗ 用 Exception 后缀
public class TomlParseFail : Exception { ... }              // ✗
```

- 与 C# / Java / Python `*Error` ≈ Rust `*Error` ≠ z42 的 `*Exception` —— z42 跟 C# 公约（更通用，"异常"比"错误"涵盖更广）
- 命名空间：通常放在 owning 包的根 namespace（`Std` 而非 `Std.Toml`），让继承 `Exception` 的 `: Exception` 引用通过 same-namespace lookup 解到 `Std.Exception`。详见 stdlib `TomlException.z42` 顶部注释

### 14b. Attribute 类型：`Attribute` 后缀强制（D8）

所有继承自 `Std.Attribute` 的类型**必须**以 `Attribute` 结尾，应用时**剥去后缀**：

```z42
public class RouteAttribute : Attribute { ... }   // ✓ 定义带后缀
[Route("/u")] class C { }                          // ✓ 应用剥后缀

public class Route : Attribute { ... }             // ✗ 缺后缀 → E0444
```

- 与 `*Exception`（§14）同族的后缀强制，但语义相反：exception 应用即真实类名，attribute **应用剥后缀**
  （类 `RouteAttribute` ↔ 用名 `[Route]`），反射引用仍用真实类名 `typeof(RouteAttribute)`。
- 比 C# 更严：C# 后缀可选（CA1710 软警告）、双拼法（`[Route]`/`[RouteAttribute]`），z42 强制单拼法。
- 机制与判据见 [attributes.md](attributes.md) 与 attribute-handler-registry design D8。

### 15. Enum 命名：单数 vs 复数

- **互斥状态枚举（普通 enum）** → **单数** 类型名
- **位标志枚举（`[Flags]` / bit-OR 组合）** → **复数** 类型名（.NET 公约）

```z42
// ✓ 互斥单数
public enum Direction { North, South, East, West }
public enum LogLevel  { Debug, Info, Warning, Error }
public enum TokenKind { Identifier, Number, String, Operator }

// ✓ 位标志复数（每个 member 仍 PascalCase 单数）
[Flags]
public enum FileAccess { Read = 1, Write = 2, ReadWrite = 3 }
[Flags]
public enum WatchEvents { Created = 1, Modified = 2, Deleted = 4 }
```

**成员命名**：永远 PascalCase（不论单 / Flags）。member 不带前缀（不要 `LogLevel.LL_INFO`、不要 `Direction.DirNorth`）。

> stdlib 现有 `ModeFlags`（Delegates）当 `[Flags]` 用但 z42 暂无 `[Flags]` attribute，靠手工 `int` + 位运算 workaround（见 `SubscriptionRefs.z42` 注释）。`[Flags]` attribute 落地后正式升级为 enum。

### 16. 反"否定式"命名

```z42
// ✗ 否定式作为名字
public bool IsNotEmpty;
public bool DisableLogging;
public void WithoutCache();
public bool IsDisabled;     // 看上下文：如果默认就 disabled，倒过来更合理

// ✓ 正向 + 调用方取反
public bool IsEmpty;        // 调用方写 !IsEmpty
public bool IsLoggingEnabled;
public void DisableCache(); // 或 EnableCache(false)
public bool IsEnabled;
```

**理由**：否定式叠加（`!IsNotEmpty`）需要读者绕两道脑筋。**默认值能不能映射到 `false`**也是判断点：`IsEnabled = true`（默认 enable）比 `IsDisabled = false`（双重否定）更自然。

**唯一可接受的否定式**：当**正向用语在领域里就是少见 / 不自然**时：

```z42
public bool IsReadOnly;     // ✓ "ReadOnly" 是单一概念，比 "IsWritable=false" 更直接
public bool IsAbstract;     // ✓ 同上
public bool IsSealed;       // ✓ 同上
```

判断准则：**领域里大家都说 "read-only" 而不是 "non-writable"** → 用 `IsReadOnly`。否则取正向。

### 17. 构造器 / 工厂方法

- **首选 `new` ctor**：直接、明确
  ```z42
  var box = new Box<int>(42);
  ```
- **`static Create(...)` 工厂**：当需要多步初始化、参数推断、或返回**子类型** / **缓存实例**
  ```z42
  var p = Process.Create(argv).WithEnv("KEY", "value").Spawn();
  ```
- **`static From<X>(X x)` 类型转换工厂**：从其他类型构造
  ```z42
  var dt  = DateTime.FromUnixMs(1700000000000);
  var color = Color.FromHex("#ff8800");
  ```
- **`static Empty()` / `Default` 常量实例**：返回 / 暴露空 / 默认实例
  ```z42
  var dict = Dictionary<string,int>.Empty();
  var color = Color.Default;        // 公开静态字段
  ```
- **不允许** `Make*` / `Construct*` / `Build*` 当通用工厂前缀 —— `Build*` 仅用于真正的 fluent builder（`new StringBuilder().Append(...).Build()`），且 `Build()` 返回的是不同类型（builder → result）

### 18. Lambda 与 discard `_`

**Lambda 参数命名**：

- 单参短名 OK：`xs.Where(x => x > 0)`, `list.Map(s => s.Length)`
- 多参语义化：`pairs.Map((key, value) => ...)`（不是 `(a, b)`）
- 嵌套 lambda 禁止同名 shadowing：

```z42
list.Map(item => item.Children.Map(item => item.Id))   // ✗ 内外 item 同名
list.Map(parent => parent.Children.Map(child => child.Id))   // ✓
```

**Discard `_`**：

- `_` 表"我不关心这个值"：
  ```z42
  var (_, value) = pair;            // 只要 value
  list.Map((_, idx) => idx);        // 只要 index
  ```
- `_` 不是 identifier，**不能** 之后引用（`_` 是模式匹配 / destructuring 的特殊 wildcard）
- **不允许** `_` 当字段 / 局部名（与"私有字段下划线前缀"撞）；私有字段必须 `_x`（至少 2 字符）

---

## 反模式（不要做）

| 反模式 | 为什么不要 | 改用 |
|--------|----------|------|
| `class _Foo` | `_` 前缀仅用于私有字段 | `internal class Foo` |
| `private int Count` | 私有应带 `_` | `private int _count` |
| `int X, Y, Z` 字段全单字母 | 仅 3D 数学 / 向量场景可（如 `Vector3.X/Y/Z`）；通用避免 | `public int Width, Height` |
| `bool valid` | 不带前缀（看似名词）| `bool isValid` |
| `interface FooInterface` | 用 `I` 前缀，别用后缀 | `interface IFoo` |
| `interface IIFooMonad` | 双 `I` 是 typo / 噪音 | `interface IFooMonad` |
| `var ll = items.Count()` | 缩写 / 单字母变量命名 | `var itemCount = ...` |
| `class TYPE_REGISTRY` | 类不用 SCREAMING_SNAKE | `class TypeRegistry` |
| `Map<K, V>` 单字母泛型参数 | z42 用 `TKey` / `TValue` | `Map<TKey, TValue>` |
| `static const int kMaxSize` | C++ 风格 `k` 前缀过时 | `static const int MaxSize` |
| `Util` / `Helper` / `Manager` / `Service` 后缀 | 含糊；改用动作或域名 | `StringFormatter`, `OrderProcessor` |
| 中英混合标识符 | 工具链 / IDE 兼容性差 | 英文 |
| 拼音 | 同上 | 英文 |
| `bool IsNotEmpty` | 双重否定 `!IsNotEmpty` 难读 | `bool IsEmpty`，调用方取反 |
| `class TomlError : Exception` | Exception 类型缺 `Exception` 后缀 | `class TomlException : Exception` |
| `enum DayOfWeeks` | 互斥状态用复数 | `enum DayOfWeek`（单数）|
| `[Flags] enum FileAccess` 单数 | bit-OR 集合用复数 | `[Flags] enum FileAccesses` 或 `FileModes` |
| `LogLevel.LL_DEBUG` | enum 成员加类型前缀 | `LogLevel.Debug` |
| `MakeFoo()` / `Construct*()` / `NewFoo()` 工厂 | 用 `Create*` / `From*` | `static Foo Create(...)`, `static Foo FromHex(s)` |
| `static Foo Build()` 返回 Foo | `Build()` 仅给真正的 builder | `static Foo Create(...)` |
| Lambda 嵌套同名 `x => x.Map(x => ...)` | shadowing 难读 | `outer => outer.Map(inner => ...)` |
| 私有字段 `private int _` | 单字符 `_` 与 discard 冲突 | `private int _count` |

---

## 与其他语言对照

| 维度 | z42 | C# | Rust | Java | Swift | Go | Python (PEP 8) |
|------|------|------|------|------|------|------|--------|
| 类型 | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` | `PascalCase` |
| 方法 | `PascalCase` | `PascalCase` | `snake_case` | `camelCase` | `camelCase` | `PascalCase` (export) / `camelCase` | `snake_case` |
| 字段（公开） | `PascalCase` | `PascalCase` (property) | `snake_case` (pub) | `camelCase` | `camelCase` | `PascalCase` | `snake_case` |
| 字段（私有） | `_camelCase` | `_camelCase` | `snake_case` | `camelCase` | `camelCase` | `camelCase` | `_snake_case` |
| 局部 / 参数 | `camelCase` | `camelCase` | `snake_case` | `camelCase` | `camelCase` | `camelCase` | `snake_case` |
| 常量 | `PascalCase` | `PascalCase` | `SCREAMING_SNAKE` | `SCREAMING_SNAKE` | `camelCase` | `PascalCase` | `SCREAMING_SNAKE` |
| 泛型参数 | `T` / `TKey` | `T` / `TKey` | `T` / `K` | `T` / `K` | `T` / `Key` | (无泛型 → generics) | `T` (typing) |
| 接口前缀 | `I*` | `I*` | 无（trait 名词）| 无 | 无（protocol）| 无（`-er` 后缀）| 无 |
| 缩略词 | `Url` / `IO` | `Url` / `IO` | 全 snake | `URL`（旧）/ `Url`（新）| `URL`（旧）/ `Url`（新）| `URL` | `URL` |
| 文件名 | `Calculator.z42` | `Calculator.cs` | `calculator.rs` | `Calculator.java` | `Calculator.swift` | `calculator.go` | `calculator.py` |

**z42 立场总结**：

- **形状**借 C# / Swift（PascalCase 类型 / camelCase 值的二元体系）
- **接口前缀 `I`** 跟 C# 而非 Swift（与 stdlib 既有约定一致 + 类/接口在使用点区分）
- **私有字段 `_` 前缀**跟 C# 主流（区分参数 / 局部）
- **常量 PascalCase** 跟 C# 而非 Rust（视觉风格统一，常量是值但与类型同级"实体"）
- **缩略词** 跟 .NET 公约（2 字母全大写，3+ PascalCase）
- **不用** Go 大小写编码可见性
- **不用** Rust snake_case 函数

---

## 何时偏离

z42 是语言层规范，规则**不强制**（不会 lint fail）。允许偏离的场景：

1. **第三方 ABI 兼容**：调用 C / native 库时，类型名要直接对应外部命名（`extern struct GLFWwindow`）。`extern` / native 边界尊重外部命名
2. **数学符号**：`Vector3.X/Y/Z`, `Matrix4.M00/M01`, `Complex.Re/Im` 等约定俗成的单字母字段
3. **DSL / generated code**：domain-specific 代码（SQL 映射、protobuf 生成）可保留源命名
4. **历史代码迁移**：从其他语言移植时可暂保留原命名，迁移完成后批量重命名

---

## Deferred / Future Work

### naming-conv-1: Async 后缀决策

L3 async / await 引入时确定 `Async` 后缀策略（C# 风格的 `LoadAsync()` 还是 Swift / Rust 的无后缀）。当前 stdlib 无 async 代码。

### naming-conv-3: z42-fmt 自动 enforce

0.2.4 z42-fmt 落地时把本文档转成 lint 规则。当前用户违反不会编译失败 —— z42-fmt 集成后可选 `--strict` 模式 enforce。

### naming-conv-5: 线程局部变量正式形态（0.8.x concurrency）

L3 concurrency 落地时 finalize 线程局部存储的语法（attribute / modifier / generic container），同时校验本规范"命名规则与静态字段相同"的承诺仍成立。预计需要 update §11。
