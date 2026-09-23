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

### 场景 5 — 初始化器抛异常 ⇒ 抛包装异常，程序终止

```z42
static class Boot { [ModuleInit] static void Init() { throw new Exception("boom"); } }
```

**期望**：触达该包时抛 `Std.TypeInitializationException`，消息形如
`the type initializer for `<ns>.$Module` threw an exception: boom`；失败状态被记住，
**不重试、不吞**。（原文此处写「`refresh_module_pending` 只把 `Done` 算作完成，门因此保持
非零」——那正是下面这条缺陷的成因，已由 fix-module-init-failure-scope 改成两个计数 +
归属判定，见下。）

> ✅ **已闭合（fix-module-init-failure-scope，2026-09-23 当日）：下面这条「已知差距」的
> 诊断是错的，保留原文只为留痕。** 真因不在 `catch`，而在屏障的**作用范围**：失败的包让
> **全程序级**的 `module_pending` 门永远非零，于是每一次调用都重抛它 —— 包括用户 `catch`
> 块里的第一条 `Console.WriteLine`。异常其实一直是被接住的，只是 handler 里又被一条无关
> 调用抛了出来（把 catch 体清空，程序立刻正常退出）。
> 修法 = `Failed` 从 `module_pending` 移到独立的 `module_failed`，重抛加**归属判定**
> （`<ns>.$Module` 只覆盖 `<ns>.` 开头的符号）。现在的行为与下面钉死的 C# 实验②逐行一致：
> `start` → `caught-1` → `caught-2` → `end`，进程正常退出。门 =
> `src/tests/cross-zpkg/module_init_failure_catchable/`；机制见
> `docs/internals/src/runtime/static-ctor-init.md`。
>
> 🔴 ~~**已知差距（实测，2026-09-23）**~~：这条异常**当前不能被用户 `catch` 捕获** ——
> 程序以未捕获异常终止。同形的**类型**初始化失败（跨包 cctor、cctor 内嵌套帧抛出）
> 都能被 `catch (TypeInitializationException)` 正常捕获，用干净 A/B 逐项排除过：
> 与调用形态（自由函数 / 静态方法）无关、与 catch 是否带类型无关、与 interp/jit 无关、
> 与屏障插入位置无关（挪进 `ensure_callee_owner_init` 内部仍不可捕获）。
> 差别只剩「这次调用内同时发生了包加载」。根因未查清 ⇒ **不为一个不成立的行为写门**，
> 详见 design.md「已知差距」。
>
> **目标行为已由 C# 实测钉死**（.NET 10，design.md 有完整记录）：`[ModuleInitializer]` 抛异常
> ⇒ `TypeInitializationException`（`<Module>`，原异常进 `InnerException`），**只要触发点落在
> `try` 内就能 `catch`**，第二次触达仍抛、不重试。我们在「依赖包 init 在 try 内被触达」这一格
> 偏离了它；（主包那条已随 E0487 禁掉 —— 现在唯一的失败路径就是依赖包这一条，所以这条差距更该修。）修它时以此为验收标准。

### 场景 6 — 没有 `[ModuleInit]` 的包：零行为变化、零字节变化

**期望**：
- 不合成 `$Module` 类型（`ir` 里不含 `$Module`）。
- 全仓现有 zpkg 重编后与本变更前**逐字节相同**（不动点对账）。

## ADDED：E0486 —— `[ModuleInit]` 标注目标非法

| 写法 | 期望 |
|---|---|
| `[ModuleInit] void Init()`（非 static） | E0486 |
| `[ModuleInit] static void Init(int x)`（有参） | E0486 |
| `[ModuleInit] static int Init()`（返回非 void） | E0486 |
| `[ModuleInit] static void Init<T>()`（泛型） | E0486 |
| `[ModuleInit] class C { }`（标在类型上） | E0486 |
| `[ModuleInit] private static void Init()` | **合法**（可见性不限） |

## ADDED：E0485 —— 一个包里出现第二个 `[ModuleInit]`

| 写法 | 期望 |
|---|---|
| 同一包两个文件各一个 `[ModuleInit]` | E0485（报后出现处，带第一处 `file:line`） |
| 同一文件两个 `[ModuleInit]` | E0485 |
| 两个**不同包**各一个 | **合法** |

🔴 E0486 与 E0485 必须是两个码：「签名不合法」与「包内重复」是两件事，
合并即一码两义（[[diagnostic-code-uniqueness-program]] 刚归位过两次）。

## ADDED：E0487 —— 可执行包里用了 `[ModuleInit]`

| 写法 | 期望 |
|---|---|
| `kind = "lib"` 的包里写 `[ModuleInit]` | ✅ 合法 |
| `kind = "exe"`（含未写 kind 的默认值）的包里写 `[ModuleInit]` | **E0487**，位置指向那处标注 |
| exe 包里写的是**非法**的 `[ModuleInit]`（签名不对） | **E0487**（一步报到位，不让用户先改完签名看 E0486） |

理由见 design.md「只对库包开放」：包初始化器在 `Main` 之前执行 ⇒ 失败无从捕获；
而 exe 有 `Main` 这个天然入口，写第一行即可且失败可控。

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
