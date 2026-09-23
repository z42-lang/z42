# 包级初始化（`[ModuleInit]`）

```z42
namespace Acme.Widgets;

static class Bootstrap {
    [ModuleInit]
    static void Init() {
        WidgetRegistry.Register("button", ButtonFactory.Create);
        WidgetRegistry.Register("slider", SliderFactory.Create);
    }
}
```

**保证**：`acme.widgets` 这个 zpkg 被加载后、**本包任何代码被执行前**，`Init()` 恰好执行一次。

与[静态构造函数](static-constructors.md)的关系：包初始化器就是一个合成伪类型
`<ns>.$Module` 的类型初始化器 —— 同一个机制，只是宿主类型由编译器合成、且触发方式不同
（见下「执行时机」）。

## 声明规则

标注目标必须是**有体、无参、返回 `void`、非泛型**的方法：

| 写法 | 结果 |
|---|---|
| `static class B { [ModuleInit] static void Init() { } }` | ✅ |
| `[ModuleInit] void Init() { }`（顶层自由函数） | ✅ —— 自由函数无 this，豁免 `static` |
| `[ModuleInit] private static void Init() { }` | ✅ —— 可见性不限，`private` 最常见 |
| 类内**实例**方法 / 有参 / 返回非 `void` / 泛型 / 标在类型上 | ❌ `E0486` |

### 一个包至多一个

```z42
// a.z42
static class BootA { [ModuleInit] static void InitA() { } }
// b.z42（同一个包）
static class BootB { [ModuleInit] static void InitB() { } }   // ❌ E0485
```

要做多件事就写在同一个方法里 —— **那样顺序是显式的**：

```z42
[ModuleInit]
static void Init() {
    LoadNativeCodecs();     // 先
    RegisterHandlers();     // 后
}
```

这条限制是故意的：与其提供一个「包内多个初始化器」的执行顺序让人去依赖（或让人去记住
「别依赖它」），不如让这种写法**无法表达**。

### 只能用在库包里

```toml
# z42.toml
kind = "lib"     # ✅ 包初始化器可用
kind = "exe"     # ❌ 里面写 [ModuleInit] ⇒ E0487
```

可执行包（`kind = "exe"`，未写 `kind` 时的默认值也是它）**不支持** `[ModuleInit]`。两个理由：

1. 它在 `Main` 之前执行 ⇒ **失败时没有任何代码能捕获它**，程序直接崩（C# 同形：entry 模块的
   module initializer 抛异常就是未捕获崩溃）。
2. exe 本来就有 `Main` 这个天然入口 —— 写进 `Main` 第一行能做同样的事，**失败还可以
   `try`/`catch`**。

```z42
void Main() {
    LoadNativeCodecs();     // ← 装配写这里；失败可控
    RegisterHandlers();
    Run();
}
```

## 执行时机

| 保证 | 说明 |
|---|---|
| **包加载后、本包任何代码执行前** | 措辞与 C# module initializer 一致 |
| **恰好一次** | 多次触达同一个包不会重复执行 |
| **跨包顺序 = 实际加载顺序** | 不排序、不预扫；包 A 的初始化器里触达包 B，B 的初始化器先跑完 |

触发与普通静态构造器不同：静态构造器等到**该类型**被首次使用，包初始化器在**这个包**被
加载后就跑 —— 哪怕你只调了包里的一个自由函数、一个类型都没碰。

```z42
using Acme.Widgets;
void Main() {
    Ping();      // 只调自由函数 —— 包初始化器照样先跑完
}
```

## 失败

初始化器抛异常 ⇒ 该包被标记为**初始化失败**，触达它的那次操作抛包装异常（可 `catch`，
对标 C# `TypeInitializationException`）。**不吞、不重试**：之后每一次触达都会再抛一次，
不会让你拿到一个半装配的包继续跑。

失败**只影响这个包**——不触达它的代码（别的包、标准库、你自己的 `catch` 块）照常运行：

```z42
using Demo.Widgets;      // 这个包的 [ModuleInit] 会抛

void Main() {
    try {
        Ping();                              // 触达 ⇒ 抛
    } catch (Exception e) {
        Console.WriteLine(e.Message);        // 不触达 ⇒ 正常执行
    }
    try { Ping(); } catch (Exception e) { }  // 再触达 ⇒ 再抛一次（不重试初始化器）
    Console.WriteLine("done");               // 不触达 ⇒ 正常执行，程序正常退出
}
```

## 什么时候**不**该用它

如果你要做的是「把一批东西登记进一张表」，**编译期收集更好** —— z42 的测试索引
（`TIDX` 段）就是这么做的：表在编译期烘焙好，运行期直接读，没有谁在启动时跑回调来注册自己。

`[ModuleInit]` 适合真正需要在运行期做的装配：打开 native 库、读环境变量、建连接池。

> ⚠️ 加载期执行用户代码是个有历史包袱的能力（C++ 的 static initialization order fiasco、
> Swift 砍掉 ObjC `+load`、Rust 拒绝把它放进语言）。z42 只承诺「本包自己的代码之前」，
> **不承诺跨包的相对顺序** —— 那由实际触达决定，换一行调用就可能变。

## 诊断

| 码 | 含义 |
|---|---|
| [`E0486`](../appendix/error-codes.md) | `[ModuleInit]` 标注目标非法 |
| [`E0485`](../appendix/error-codes.md) | 一个包里出现了第二个 `[ModuleInit]` |
| [`E0487`](../appendix/error-codes.md) | 在可执行包（`kind = "exe"`）里用了 `[ModuleInit]` |
