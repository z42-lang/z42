# Spec: 静态初始化（本变更的 delta）

> 可验证场景。每条都要对应 tasks.md 里至少一项验证任务。
> 长期规范落点：`docs/reference/src/language/static-constructors.md` + `static-members.md`。

## ADDED：统一的类型初始化器

### 场景 1 — 无显式静态构造器的类，初始化器在首次使用前执行

```z42
class Counter {
    public static int Value = Compute();
    static int Compute() { Console.WriteLine("init"); return 7; }
}

void Main() {
    Console.WriteLine("before");
    Console.WriteLine(Counter.Value);
}
```

**期望输出**：
```
before
init
7
```

改前：`init` 先于 `before`（包加载后批量跑）。**这是本变更的核心可观察差异。**

### 场景 2 — 从未使用的类型，其初始化器不执行

```z42
class Never {
    public static int X = Boom();
    static int Boom() { Console.WriteLine("SHOULD NOT RUN"); return 1; }
}

void Main() { Console.WriteLine("done"); }
```

**期望输出**：`done`，且**不含** `SHOULD NOT RUN`。

### 场景 3 — 四个触发点对无显式 cctor 的类同样成立

对 `class T { public static int F = 1; public static int M() { return 2; } }`，
以下每一个都必须触发 T 的初始化：读 `T.F` / 写 `T.F = x` / `new T()` / 调 `T.M()`。

**验证方式**：四个独立 fixture，各自断言初始化器恰好执行一次（用副作用计数）。

## MODIFIED：跨类型初始化顺序

### 场景 4 — 依赖顺序压倒声明顺序

```z42
class A { public static int X = B.Y + 1; }   // 声明在前
class B { public static int Y = 41; }        // 声明在后

void Main() { Console.WriteLine(A.X); }
```

**期望输出**：`42`

**改前实测**（2026-09-22，当前 main 自建 z42c + z42vm，`a50f7e897`）：输出 `A.X = 1`
—— `A.X` 在 `__static_init__` 里按声明序先于 `B.Y` 执行，读到零值，**静默给出错值**。

三组对照确立根因就是"走哪条管道"：

| 源程序 | 走的管道 | 实测 |
|---|---|---|
| A 在前，B 只有字段初始化器 | `__static_init__` | **`A.X = 1`** ❌ |
| B 在前，B 只有字段初始化器 | `__static_init__` | `A.X = 42` ✅ |
| A 在前，**B 写了显式 `static B()`** | 类型初始化器 | `A.X = 42` ✅ |

**第三行是决定性的**：同一个源程序，只因为 B 多写了一个空壳 `static B()`，结果就从 1 变成 42。
本变更要统一到的那条管道（类型初始化器）行为本来就是对的，被删掉的那条是错的。

引用类型更隐蔽——`static string X = B.Y` 同样条件下得到 `A.X = [null]` 而 `B.Y = [initialized]`，
**没有任何报错**。

### 场景 5 — 规范不承诺无依赖类型之间的相对顺序

```z42
class P { public static int A = Log("P"); }
class Q { public static int B = Log("Q"); }
```

P 与 Q 互不依赖 ⇒ **不承诺** `P` 先于 `Q`。本场景不写断言顺序的门，只在 reference
里明确"不承诺"，防止将来有人把实现细节当契约。

## MODIFIED：初始化失败从终止进程改为可捕获异常

### 场景 6 — 静态字段初始化器抛出

```z42
class Bad {
    public static int X = Throw();
    static int Throw() { throw new Exception("boom"); }
}

void Main() {
    try { Console.WriteLine(Bad.X); }
    catch (TypeInitializationException e) { Console.WriteLine("caught: " + e.Message); }
}
```

**期望输出**：`caught: boom`（原始消息在 `Message` 里）

改前：进程以 `uncaught exception in static init ...` 直接终止，**无法捕获**。

### 场景 7 — 失败的类型后续访问一律重抛，不重试

承接场景 6：第二次读 `Bad.X` 仍抛 `TypeInitializationException`，且 `Throw()` **不再执行**
（副作用计数 == 1）。

## REMOVED：`__static_init__`

### 场景 8 — 产物中不再存在 `__static_init__` 函数

**验证方式**：编译任一含静态字段初始化器的包，断言产出 zpkg 的 SIGS 段中
**没有**任何以 `.__static_init__` 结尾的函数名。

这一条是**会变红的门**，防止机制"删了一半"。

## MODIFIED：屏障成本

### 场景 9 — 编译期可证 init-free 的站点不发射屏障

对 `class NoStatics { public int F; }` 的 `new NoStatics()` / 静态方法调用站点，
IR 层断言屏障位 `owner_init_free == true`。

**验证方式**：`z42c --dump-ir` 断言站点标志位。

### 场景 10 — 跨包站点一律不置 init-free 位

一个引用依赖包类型的站点，无论该类型在编译时看起来有没有初始化器，
`owner_init_free` 必须为 `false`。

**验证方式**：cross-zpkg fixture + IR dump 断言。这条防的是依赖换版本后编译期结论过期
导致的**静默跳过初始化**。

### 场景 11 — 增量编译不留过期结论

同包 A 文件的站点引用 B 文件的类 `C`；先编译一次（`C` 无静态字段 ⇒ 站点置 init-free），
再给 `C` 加一个静态字段初始化器后**增量**重编译。

**期望**：A 文件站点的 `owner_init_free` 变回 `false`（整包装配时重算全部站点，
不是只置位 / OR）。

## 不在本变更范围

- 包级 `[ModuleInit]` 回调 —— 后继变更 `add-module-init-hook`。
- 惰性加载触发点集合（T1 函数查找 / T2 类型查找 / T3 静态字段引用）本身不变。
