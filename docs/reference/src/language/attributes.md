# 自定义 Attribute 与反射

> 对齐：2026-09-17

attribute 是**用户自定义的元数据注解**：贴在声明上，运行期经反射读回**活实例**。
语法取自 C#，但修掉了 C# 的几处长期缺陷（见下「对 C# 的改进」）。

## 定义与应用

attribute 是一个继承 `Std.Attribute` 的普通类，**类名必须以 `Attribute` 结尾**，
应用时**剥去后缀**——类 `RouteAttribute` 写作 `[Route]`：

```z42
using Std;
using Std.Reflection;

class RouteAttribute : Attribute {
    public string Path;
    public string Method;
    // 全部状态走构造器（命名实参 + 默认值）——单一初始化路径
    public RouteAttribute(string path, string method = "GET") {
        this.Path = path;
        this.Method = method;
    }
}

[Route("/users", method: "POST")]
class UsersController { }

void Demo() {
    Type t = typeof(UsersController);

    // 全部 attribute：活实例，按应用顺序
    Attribute[] all = t.GetCustomAttributes();   // [ RouteAttribute 实例 ]

    // 按类型单查（不存在返回 null）——反射用**真实类名**（带后缀）
    RouteAttribute r = (RouteAttribute) t.GetAttribute(typeof(RouteAttribute));
    Console.WriteLine(r.Path);     // "/users"
    Console.WriteLine(r.Method);   // "POST"

    // 缓存：对同一个 Type 重复调用返回同一批实例
    Attribute[] again = t.GetCustomAttributes();
}
```

规则：

- attribute 的实参限**编译期常量**（字面量 / enum 成员 / `typeof`）。
- 应用位剥后缀：`[Route]` 被展开成 `RouteAttribute` 去解析。缺后缀的 `: Attribute` 类
  （`class Route : Attribute`）报
  `E0444: attribute class 'Route' must end with 'Attribute' suffix`。
- 反射查询用**真实类名**（`typeof(RouteAttribute)`）。`GetAttribute(Type)` 按实例的运行期类型
  （`FullName`）比较，所以**子类 attribute 也会匹配**。

## 可以贴在哪、怎么读回来

五种反射载体都有同一对 API：

| 载体 | 取全部 | 按类型单查 | 贴法 |
|---|---|---|---|
| `Std.Type` | `GetCustomAttributes() : Attribute[]` | `GetAttribute(Type) : Attribute?` | `[X] class C { }` |
| `MethodInfo` | 同上 | 同上 | `[X] public void M() { }`（含顶层函数）|
| `FieldInfo` | 同上 | 同上 | `[X] public int f;`（实例 + 静态）|
| `ParameterInfo` | 同上 | 同上 | `void M([X] int p)` |
| `PropertyInfo` | 同上 | 同上 | `[X] public int P { get; set; }` |

全部**缓存**：同一个反射对象上重复调用 `GetCustomAttributes()` 返回同一批实例。

```z42
Type t = typeof(Service);
foreach (MethodInfo m in t.GetMethods()) {
    if (m.Name == "List") {
        Attribute[] attrs = m.GetCustomAttributes();
        DocAttribute d = (DocAttribute) m.GetAttribute(typeof(DocAttribute));
        ParameterInfo[] ps = m.GetParameters();
        Attribute[] pa = ps[0].GetCustomAttributes();
    }
}
foreach (FieldInfo f in t.GetFields())     { f.GetCustomAttributes(); }
foreach (PropertyInfo p in t.GetProperties()) { p.GetCustomAttributes(); }
```

> ⚠️ **属性只有自动属性能带 attribute**。`PropertyInfo.GetCustomAttributes()` 读的是自动属性
> 脱糖出来的那个私有 backing 字段上的 attribute；**计算属性**（写了 `get { ... }` 体、没有
> backing 字段）永远返回空数组。

## 对 C# 的改进

| # | C# 的问题 | z42 |
|---|---|---|
| 1 | `Attribute` 后缀**可选** + 双拼法（`[Foo]` 与 `[FooAttribute]` 都行，无后缀类也合法）| **后缀强制、单拼法**：类名必须 `*Attribute`（否则 `E0444`），应用只接受剥后缀的 `[Foo]` |
| 2 | 双初始化路径（positional → ctor，named → 公开字段直写）| **单一构造器路径**：全部实参走 ctor，复用命名实参 + 默认值 |
| 3 | 每次 `GetCustomAttributes()` 重新分配实例，返回 `object[]` | **缓存**：首次实例化，之后返回同一批；返回类型是 `Attribute[]` |
| 4 | 实例可变（正是 #3 必须每次复制的根因）| **实例在 ctor 内一次写定**，缓存因此安全 |
| 5 | `AttributeUsage` 是自循环的元属性 + 反直觉默认值 | **暂不提供**；将来做成一等声明子句，不做元属性 |

**刻意保留的 C# 约束**：实参限编译期常量（安全 + 「attribute 是数据不是行为」的心智模型）；
attribute 是被动的——它只是数据，不会主动改写被注解的声明。

**暂未提供**：`AttributeUsage` 式的 target / 重复性限制（attribute 可以贴在任何支持的位置）；
泛型 attribute 类与 `GetAttribute<T>()` 泛型糖（用 `GetAttribute(typeof(T))`）；
C# `CustomAttributeData` 那种「不实例化就读原始 ctor 实参」的视图。

## 局部抑制诊断：`#suppress` / `[Suppress]`

关闭某条诊断规则，对应 C# 的 `#pragma warning disable/restore` 与 `[SuppressMessage]`。
两者都是**纯编译期**的，不写进产物：

```z42
#suppress Z9002 "这段是生成代码"       // 区间起
try { risky(); } catch (Error e) { }   // 此处 Z9002 被抑制
#restore Z9002                          // 区间止（省略则延伸到文件尾）

[Suppress("Z9002")] void Hot() { }      // 声明级：Hot 及其内部抑制 Z9002
```

- `#suppress <Id> ["reason"]` / `#restore <Id>` 按**源码区间**生效，规则 Id **精确匹配、无通配**
  （通配与项目级 severity 覆盖归 `z42.toml` 的 `[lints]` 段，不在本页）。
- `[Suppress("<Id>", "reason")]` 按**声明子树**生效，比区间精确。
- `Suppress` 是保留名，不能再定义同名的自定义 attribute 类。

## 编译期 caller 宏

对齐 C# 的 `[CallerMemberName]` / `[CallerLineNumber]` / `[CallerFilePath]`——四个内建的编译期宏，
**只能用作可选参数的默认值**；调用点省略该实参时，编译器注入**调用方的**上下文：

```z42
class Log {
    public static void info(string msg,
                            string member = caller_member!(),   // 调用方的 enclosing 成员名
                            int    line   = caller_line!(),     // 调用点行号
                            string file   = caller_file!(),     // 调用点源文件
                            string mod    = module_path!()) {   // 调用点命名空间
        Console.WriteLine(msg + " @" + member + ":" + line.ToString());
    }
}

void run() {
    Log.info("hello");                 // member="run"、line=本行、file=本文件、mod=本命名空间
    Log.info("bye", "customCaller");   // 显式实参覆盖对应的 caller 默认值
}
```

- 写法是 `name!()`。**只有这四个宏名合法，且只合法于参数默认值位**。宏名写错 / 参数类型不符
  （`member`、`file`、`mod` 须 `string`，`line` 须 `int`）/ 用在普通表达式位 → `E0450`。
- **跨包也按调用点注入**：调用一个别的包里的方法时注入的是**你这边**的上下文，不是库定义处的。
- 反射里 caller 参数的 `ParameterInfo.DefaultValue` 是 `null`——它没有固定值，值由调用点决定。
- 目前只做**单层**注入；没有 Rust `[track_caller]` 那样的多层传播。

另一个编译期宏 `available!(...)` 的合法位置相反（只能用在普通表达式位），见
[`available!()` 符号可用性探测](available-macro.md)。

## 相关

- [`[Record]` 与主构造器](record-attribute.md)——编译器内建 attribute 的一例
- [`[Forward]`（成员转发）](member-forwarding.md)
- [`available!()` 符号可用性探测](available-macro.md)
- [错误码表](../appendix/error-codes.md)——`E0444` / `E0450` 等的完整条目
