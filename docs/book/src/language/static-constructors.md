# 静态构造函数

```z42
class Config {
    public static readonly int MaxRetries = 3;
    public static string Endpoint;

    static Config() {
        Endpoint = LoadEndpointFromNative();
    }
}
```

语义对标 C#：**每个类型至多执行一次**，在**首次使用该类型之前**。

## 执行时机

静态构造器是**惰性**的——程序启动时不跑，直到该类型被首次使用。触发点四个：

| 触发 | 例 |
|---|---|
| 读该类型的静态字段 | `Config.Endpoint` |
| 写该类型的静态字段 | `Config.Endpoint = x` |
| 创建该类型的实例 | `new Config()` |
| 调用该类型的静态方法 | `Config.Reload()` |

一个从没被用到的类型，它的静态构造器**不会执行**。

## 与静态字段初始化器的关系

一个类**写了**静态构造器时，它的静态字段初始化器和构造器体合成**同一个类型初始化器**，
**字段初始化器在前**：

```z42
class C {
    public static int V = 1;                 // ① 先跑
    static C() { C.V = C.V + 41; }           // ② 后跑 → V == 42
}
```

一个类**没写**静态构造器时，静态字段初始化器走按编译单元的**急切**初始化（程序启动时）。
这与 C# 一致：C# 里只有显式写了静态构造器的类才失去 `beforefieldinit`，才要求精确的
「首次使用前」；只有字段初始化器的类，运行时可以任意提前初始化。

**这条差异有实际后果**：给一个类加上静态构造器，会把它的静态字段初始化从「启动时」
变成「首次使用时」。如果你依赖某个静态字段在启动时就绪（例如它有副作用），加静态构造器
会改变行为。

## `static readonly`

静态构造器里可以给本类的 `static readonly` 字段赋值（同 C#）：

```z42
class C {
    public static readonly int V;
    static C() { V = compute(); }        // ✅
}

class D {
    static D() { C.V = 1; }              // ❌ E0415：只能赋值**本类**的
}
```

## 异常

静态构造器抛出异常时，该类型被标记为**失败**：

- 抛出的异常被包装成 [`Std.TypeInitializationException`](#)，原始消息在 `Message` 里
- **后续每一次**触发初始化的访问都再次抛出同一个异常
- 静态构造器**不会重试**

```z42
try { Console.WriteLine(C.V); }
catch (TypeInitializationException e) { /* ... */ }
```

## 重入

静态构造器里直接或间接又用到本类型时，**直接放行**（不死锁），此时可能看到**部分初始化**
的状态。这与 C# 的处理一致。

```z42
class C {
    public static int A = 1;
    public static int B;
    static C() { B = C.A; }   // 读本类型 → 放行，读到已赋值的 A
}
```

## ⚠️ 并发上的已知差距

当前实现里，若**另一个线程**正在跑某类型的静态构造器，本线程**不阻塞等待**，而是直接继续
——因此可能读到部分初始化的状态。C# 保证在这种情况下阻塞到初始化完成。

不做跨线程等待，是因为在持有解释器帧时阻塞极易与既有的静态初始化排空逻辑互相死锁。
**单线程程序不受影响**；多线程下若某个静态字段必须在首次读到时已完全初始化，请自行加同步。

## 实现

机制（按类型状态机、四个触发点的屏障、屏障为何几乎免费、两个后端如何共用一份实现）见
[静态构造函数的按类型初始化](../runtime/static-ctor-init.md)。
