# Spec: 包级初始化（本变更的 delta）

> 可验证场景。每条都要对应 tasks.md 里至少一项验证任务。
> 长期规范落点：`docs/reference/src/language/module-initializers.md`（新页）
> + `docs/internals/` 的初始化机制页。

## ADDED：`[ModuleInit]` 包级初始化回调

### 场景 1 — 包被加载即执行，先于本包任何代码

包 `lib`：
```z42
namespace Lib;

static class Boot {
    public static string Trace = "";
    [ModuleInit]
    static void Init() { Console.WriteLine("module-init"); }
}

public static class Api {
    public static void Hello() { Console.WriteLine("hello"); }
}
```

主程序：
```z42
void Main() {
    Console.WriteLine("before");
    Lib.Api.Hello();
}
```

**期望输出**：
```
before
module-init
hello
```

`module-init` 必须在 `hello` **之前**（包加载发生在首次跨包解析处，早于函数体执行）。

### 场景 2 — 只调用自由函数也触发（纯惰性方案漏掉的那条路）

包 `lib` 的自由函数（不属于任何类）：
```z42
namespace Lib;
public void Ping() { Console.WriteLine("ping"); }

static class Boot { [ModuleInit] static void Init() { Console.WriteLine("module-init"); } }
```

主程序：`void Main() { Lib.Ping(); }`

**期望输出**：
```
module-init
ping
```

🔴 **这一条是本变更相对「纯惰性」方案的关键差异**：cctor 屏障不覆盖自由函数
（`ensure_callee_owner_init` 推不出 owner 类型），只有「加载即触发」能给出这条保证。

### 场景 3 — 至多执行一次

包 `lib` 的 init 递增一个静态计数；主程序多次跨包调用（类型 + 自由函数 + 静态字段各一次）。

**期望**：计数恒为 `1`。

### 场景 4 — 包内出现第二个 `[ModuleInit]` ⇒ E0485

同一包的两个文件 `a.z42` / `b.z42` 各有一个 `[ModuleInit]`。

**期望**：编译失败，**E0485** 报在后出现的那处，消息里带上第一处的 `file:line`。

同文件内两个 `[ModuleInit]` 同样 E0485。**跨包**各有一个则完全合法（场景 7）。

### 场景 4b — 增量编译下仍能发现重复

先编一个只有 `a.z42` 带 `[ModuleInit]` 的包（成功），再给 `b.z42` 加一个 `[ModuleInit]`
后**增量**重编。

**期望**：仍报 E0485。🔴 这条守的是「只重编一个 CU 时看不见别的 CU」这个漏报形状。

### 场景 7 — 跨包各自一个，按实际加载顺序执行

包 `libA` / `libB` 各有一个 `[ModuleInit]`，分别打印 `A-init` / `B-init`。
主程序先触达 `libB`、再触达 `libA`。

**期望输出**：`B-init` 先于 `A-init`（= 实际加载顺序，不是清单声明顺序、不是字典序）。

### 场景 5 — 初始化器抛异常 ⇒ 包标记失败，后续触达抛包装异常

```z42
static class Boot { [ModuleInit] static void Init() { throw new Exception("boom"); } }
```

**期望**：主程序首次触达该包即抛（可 `catch`）的类型初始化异常，消息含内层 `boom`；
第二次触达**仍抛**（不重试、不吞）。

### 场景 6 — 没有 `[ModuleInit]` 的包：零行为变化、零字节变化

**期望**：
- 不合成 `$Module` 类型（`ir` 里不含 `$Module`）。
- 全仓现有 zpkg 重编后与本变更前**逐字节相同**（不动点对账）。

## ADDED：E0484 —— `[ModuleInit]` 标注目标非法

| 写法 | 期望 |
|---|---|
| `[ModuleInit] void Init()`（非 static） | E0484 |
| `[ModuleInit] static void Init(int x)`（有参） | E0484 |
| `[ModuleInit] static int Init()`（返回非 void） | E0484 |
| `[ModuleInit] static void Init<T>()`（泛型） | E0484 |
| `[ModuleInit] class C { }`（标在类型上） | E0484 |
| `[ModuleInit] private static void Init()` | **合法**（可见性不限） |

## ADDED：E0485 —— 一个包里出现第二个 `[ModuleInit]`

| 写法 | 期望 |
|---|---|
| 同一包两个文件各一个 `[ModuleInit]` | E0485（报后出现处，带第一处 `file:line`） |
| 同一文件两个 `[ModuleInit]` | E0485 |
| 两个**不同包**各一个 | **合法** |

🔴 E0484 与 E0485 必须是两个码：「签名不合法」与「包内重复」是两件事，
合并即一码两义（[[diagnostic-code-uniqueness-program]] 刚归位过两次）。

## UNCHANGED（显式声明不变的部分）

- 跨包初始化顺序 = **实际加载顺序**，不排序、不预扫（与「按真实依赖拓扑序」同一性质）。
- 普通类型的类型初始化器仍然**惰性**：首次使用该类型之前（`unify-static-init-into-cctor`
  的语义不变）。`$Module` 是唯一被主动触发的初始化器。
- zbc / zpkg 格式版本不变（复用 `$Cctor` 哨兵）。
- 无 `[ModuleInit]` 的程序，热路径（调用 / 静态字段访问 / `new`）指令与代价不变。

## NOT IN SCOPE

- `[ModuleShutdown]` 及其它包级回调。
- 包初始化器之间的声明式依赖（androidx.startup `dependencies()` 形状）。
- 包内多个 `[ModuleInit]` 的定序 —— 已被「至多一个」消灭。
- 「程序启动前跑遍依赖闭包」的急切语义（备选方案，已否，见 design.md）。
