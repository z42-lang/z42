# Proposal: 包级初始化回调 `[ModuleInit]`

> 状态：DRAFT（2026-09-22 起草）｜类型：`lang`（新语义）+ `vm`（新执行时机）｜完整流程
> 前置：`unify-static-init-into-cctor`（#742/#751/#758，已闭合）——**本变更的地基**

## Why

z42 今天没有「包级一次性装配」这个概念。想在包被用起来之前做一件事（填注册表、加载 native
依赖、装全局配置），唯一的手法是把它塞进**某个类**的类型初始化器，然后祈祷有人碰那个类：

```z42
static class Registry {
    public static StrMap Handlers = _build();   // 只有谁读 Registry.Handlers 才会跑
}
```

这条路有两个真问题：

1. **触发条件是「谁碰了哪个类」，不是「这个包被用了」** —— 装配代码的作者无法表达自己的意图，
   只能挑一个"大概会被碰到"的类挂上去。挂错了就是静默的空注册表。
2. **顺序不可表达** —— 一个包里若有多处装配，它们之间没有任何可声明的先后。

C# 有 `[ModuleInitializer]`、Go 有 `func init()`、OSGi 有 `Bundle-Activator`：这是个成熟形状。
z42 缺它。

**为什么是现在**：`unify-static-init-into-cctor` 之前，z42 有**两套**静态初始化机制并存
（per-CU `__static_init__` 急切 + 类型初始化器惰性），在这上面再加第三个包级概念会是净负债。
那次收敛（#742/#751/#758）之后全系统只剩**一个**初始化概念 = 每类型的类型初始化器，
于是包初始化器可以**零新概念**地落地：

> `[ModuleInit]` = 合成一个 `<pkg>.$Module` 伪类型，它的类型初始化器就是包初始化器。

这正是 C# 的做法——CLR 的 module initializer 本来就是 `<Module>` 伪类型的 `.cctor`。

## What（用户可见）

```z42
// 包 acme.widgets 的任一源文件
static class Bootstrap {
    [ModuleInit]
    static void Init() {
        WidgetRegistry.Register("button", ButtonFactory.Create);
        WidgetRegistry.Register("slider", SliderFactory.Create);
    }
}
```

**保证**：`acme.widgets` 这个 zpkg 被加载后、本包任何代码被执行前，`Init()` 恰好执行一次。

- 标注目标：`static`、无参、返回 `void` 的方法。可见性不限（`private` 最常见）。
- **一个包至多一个** `[ModuleInit]`（User 裁决 2026-09-22）。第二个 ⇒ 编译错误 **E0485**
  （报在后出现的那处，并指出第一处的声明位）。
  ⭐ 这条约束顺带消灭了「包内多个 init 谁先跑」这个问题——**让错误写法无法表达**，
  而不是写一条"别依赖顺序"的纪律让人去遵守。
- 抛异常 → 该包标记为初始化失败，后续对本包的触达抛包装异常（对标 C#
  `TypeInitializationException`，复用既有 cctor 失败语义）。

## Scope

**做**：

- `[ModuleInit]` attribute 的识别、校验、诊断（**E0484**：签名不合法）。
- 编译器合成 `<pkg>.$Module` 伪类型 + 其类型初始化器（依次调用本包全部 `[ModuleInit]` 方法）。
- 运行期在**包加载的 4 个既有收口点**（锁已释放处）主动触发 `<pkg>.$Module` 的初始化。
- 主包（不经惰性加载器）在启动路径补一处触发。
- 会变红的门：编译期 fixture + 负例、运行期跨包 e2e（含「只调自由函数」这条路）、
  「无 `[ModuleInit]` 的包零变化」的不动点对账。

**不做**（显式排除）：

- `[ModuleShutdown]` / 其它包级回调 —— 本变更只落一个钩子；升级到「接口 + 清单指名」
  （OSGi / androidx.startup 形状）的判据与路径写进 design.md，不提前付成本。
- 包初始化器之间的**声明式依赖**（androidx.startup 的 `dependencies()`）—— 依赖由「谁触达谁」
  天然给出拓扑序，不引入第二套排序机制。
- 包内多个 `[ModuleInit]` 的定序 —— 已被「至多一个」消灭，不存在这个问题。
- 格式 bump —— 复用既有 `$Cctor` 哨兵，zbc / zpkg 版本不动。

## 已裁决（User，2026-09-22）

| 问题 | 裁决 |
|---|---|
| **执行保证** | **zpkg 首次加载时就执行** —— 不是「没人触达就不跑」的纯惰性 |
| **声明形状** | `[ModuleInit]` 标注静态无参方法（C# / Go 形状），kind 判定并入 handler registry 三路判定 |
| **包内数量** | **至多一个**；多于一个是编译错误（E0485），不定序、不合并 |
| **跨包顺序** | 按**实际加载顺序** —— 不排序、不预扫，由真实触达决定 |

## 风险（必须在文档里写明）

1. **#418 批准惰性化的前提在此失效。** 当初批准「类型初始化器惰性执行」的依据是
   「已扫描 stdlib 31 个初始化器全是纯表构造、**无副作用**」。`[ModuleInit]` 的全部用途
   就是写副作用 —— 本变更等于正式邀请用户在初始化路径上写副作用。
   缓解：`$Module` **不走惰性**，它在加载收口点被主动触发（这正是 User 裁决的那条）。
2. **加载期执行用户代码**是 C++ static init fiasco 家族的风险源；Swift 砍掉 ObjC `+load`、
   Rust 拒绝把它放进语言，都是这个原因。缓解：语义只承诺「本包自己的代码之前」，
   **不承诺跨包顺序**；跨包顺序由真实触达决定。
3. **若目标是「注册」而非「初始化」，编译期收集更优** —— z42 已有这个形状（`TIDX` 段是
   编译期烘焙的测试索引，运行期直接读表，没有谁跑回调来注册自己）。design.md 记下这条判据，
   避免 `[ModuleInit]` 被当成万能锤。
