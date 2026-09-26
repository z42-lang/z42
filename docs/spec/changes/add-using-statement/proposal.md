# Proposal: `using` 语句 —— 第一个「经语法扩展机制」落地的新语法

> 状态：🔵 DRAFT 待审批 ｜ 类型：lang（需规范先行）｜ 创建：2026-09-26
> 前史：`foreach-protocol-pluggable-syntax-program` 的**批 3/4**。
> 批 0 = #783（清幻影特性）· 批 1 = #841（L0 协议表 + `[syntax]` 接线）· 批 2 = #847（L2 规则表）。
> 批 1 的 **D2 裁决**：`using` 按内建支持，但**经语法扩展机制实现**（表项 + 分派，**不是新增 if 分支**）；
> **D4**：两种形态都要（`using (r) { }` + `using var r = ...;`）。批 2 把那个机制建好了，本批是它的第一个客户。

## Why

### 1. 零件全齐，只差这一个语法

- `try/finally` 已覆盖五条退出路径（正常落出 / return / break / continue / throw）；
- **foreach 的枚举器路径已经在 binder 里合成 try/finally + 条件 `Dispose`** —— `using` 要的就是同一套，
  且比它更简单（没有 `MoveNext`/`Current`、没有元素类型推导）；
- `IDisposable` 两侧都有（stdlib 声明 + prelude 内建声明）；
- 批 2 之后，加一条语句 = **`StmtRules` 加表项 + 实现分派**，D2 的「非新增 if 分支」有了落点。

⇒ **零新 IR 指令、零 zbc/zpkg 格式 bump。**

### 2. 现状实测：资源释放根本没走异常路径（这是本批的真正理由）

2026-09-26 全仓统计（排除 `artifacts/`）：

| 项 | 数 |
|---|---|
| `.Dispose()` 文本出现 | 199 |
| ├ 文档注释里的用法示例 | 24 |
| ├ 测试里 | 153 |
| └ **生产代码真实调用点** | **22** |
| 全仓 `finally { … Dispose … }` | **4**（2 个测试 + **2 个是编译器给 foreach 合成的那段**）|
| **22 个生产调用点里落在 `finally` 的** | **0** |

⇒ 没有任何一处生产代码用 `try/finally` 保证释放。实际写法长这样
（`z42.net/src/Http/HttpServer.z42:84-91`，同一形状在该文件里重复约 6 次）：

```z42
try {
    NetworkStream stream = peer.GetStream();
    this._dispatchOne(stream, handler);
    peer.Dispose();                                    // 正常路径释放
} catch (Exception e) {
    try { peer.Dispose(); } catch (Exception e2) { }   // 异常路径再释放一次
    throw e;                                           // 手工重抛
}
```

`using (peer) { … }` 是这 6 行的一行版本，且不需要 catch-重抛（`throw e` 这种手工重抛还会损失
原始抛出点信息）。另有 13 处 `try { x.Dispose(); } catch { /* swallow */ }` 的防御式写法 ——
它们防的是「`Dispose` 自己抛」，与本批正交，**不在本批范围**（见 Non-goals）。

### 3. `using_stmt` 曾经是一个**幻影特性**，挂了三年

`LanguageFeatures` 那张表长期宣告 6 个**不存在**的特性，`using_stmt` 是其中之一
（#783 于 2026-09-23 删除）。也就是说：这个语法的设计意图一直在，还被误以为已实现。
本批让那个名字**以真名回来** —— 而按批 2 立下的纪律，加名字必须同时具备
「parser 真有消费路径」+「一条会红的门」，两者本批都给。

## What Changes

| 形态 | 语义（镜像 C#）|
|---|---|
| `using (expr) { … }` | 求值 `expr` → 进块 → 块结束（任一退出路径）调 `Dispose()` |
| `using (T v = expr) { … }` | 同上，且 `v` 在块内可见（C# 的完整形态之一）|
| `using var v = expr;` | `v` 的作用域 = **到所在块末尾**，块退出时调 `Dispose()`（C# 8 简化形态）|

降糖目标（全部是既有形状）：

```
using (e) { B }   ⇒   { var $tmp = e; try { B } finally { $tmp.Dispose(); } }
```

外加：
1. **`StmtRules` 表项**：`using` 进关键字语句表，`Feature = "using_stmt"`（第 1 层的新真名）；
2. **语句位拦截要改**：今天语句位的 `using` 被当成「放错位置的 import」拦下并报
   `DeclarationInStatement`（批 2 前就有）—— 现在它是合法语句，**拦截判据必须能区分两者**；
3. **`ProtocolNames` 复用**：`Dispose` / `IDisposable` 常量已在批 1 收好，直接引用；
4. **判定复用**：「这个类型可 dispose 吗」的判据与 foreach 那条**必须同源**
   （#827 的教训：查直接成员表会漏掉继承来的 `Dispose`）。

## Non-goals

- **不做 `await using` / `IAsyncDisposable`**：z42 没有 async。
- **不做 `Dispose` 自己抛异常的语义细化**（C# 的 `try { Dispose() } catch` 吞不吞、与主异常
  怎么合并）—— 上面那 13 处防御式写法属这一类，独立一条。
- **不做 `using` 的多资源形态** `using (a, b) { }`（C# 允许 `using (A a = …, b = …)`）——
  可用嵌套表达，先不进语法。
- **不碰后端**：零新 IR 指令、零格式 bump。
- **不改 `using` 作 import / 别名 / `global using` 的任何行为**。
- **不在本批的 support 阶段让编译器自身/stdlib 使用 `using`** —— 那是批 4（见下）。

## 分两批落地（bootstrap-seed 的硬纪律，不可合并）

`bootstrap-seed.md:122-129`「support 与 use 必须分两个 release」：

| 批 | 内容 | 能否与上一批同 PR |
|---|---|---|
| **3（support）** | lexer/parser/binder/codegen 支持 `using`；**z42c 自身源码 + stdlib + xtask 一处都不用它**；门与文档齐 | — |
| **4（use）** | 新 nightly 发布**之后**，才在 z42c / stdlib（`HttpServer` 那 6 处是首选客户）/ 示例里真的用 | ❌ 必须等一个 nightly |

⚠️ 违反的后果是**死锁**：上一个 nightly 的 z42c 不认 `using`，而当前源码用了它 ⇒ 没有编译器能编这份源码。

## 待裁议题（见 design.md）

| # | 议题 |
|---|---|
| D3-1 | **判定用名义还是形状**：要求类型**名义实现 `IDisposable`**（C# 的口径），还是只要**有 `Dispose()` 方法**（z42 的 foreach 今天就是形状判定）？两者今天在仓里会给出不同答案 |
| D3-2 | 三种形态本批做几种（D4 已定「两种都要」，但 `using (T v = expr)` 这第三种要不要一起） |
| D3-3 | `using var` 的作用域与释放顺序（多个 `using var` 时是否严格逆序释放） |
| D3-4 | 语句位 `using` 的**消歧**：`using (x) { }` vs `using Foo.Bar;`（import）—— 前瞻判据怎么写才不误伤 |
| D3-5 | `null` 目标的语义（C# 是「null 则跳过 Dispose」）；以及不可 dispose 的类型报哪个新码（E0498/E0499 可用）|
| D3-6 | 特性名用 `using_stmt`（让幻影名以真名回来）还是别的 |

## 相关

- `bootstrap-seed.md:110-135`：support/use 分两个 release 的纪律（本批切分的依据）。
- 批 1 #841 的 D2/D4 裁决；批 2 #847 建的 `StmtRules` / `StmtStep` / `FeatureNames`。
- `fix-foreach-dispose-inherited`（#827）：「可 dispose 吗」这个判据**必须问继承面** ——
  查直接成员表会让继承来的 `Dispose` 被静默跳过（响亮错误变成悄悄不释放）。本批复用同一判据。
- 诊断码：main 上已到 E0497，三个在飞 PR（#845/#850/#851）均不占新码 ⇒ **E0498/E0499 可用**
  （合并前按 `diagnostic-code-uniqueness-program` 的纪律逐个在飞分支重查）。
