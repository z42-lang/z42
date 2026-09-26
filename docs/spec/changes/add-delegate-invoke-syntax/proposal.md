# Proposal: 委托值上的 `.Invoke(args)` 显式调用语法

## Why

参考手册**承诺了一件编译器做不到的事**，而且把它当成官方模板在教。

`docs/reference/src/language/delegates-events.md`：

- `:61` —— `| 调用 | `f(arg)` 等价 `f.Invoke(arg)` |`
- `:62` —— 「触发前务必查空：`var h = X; if (h != null) { h.Invoke(args); }`」
- `:264-266` —— 单播 `event Action<T>` / `event Func<..>` / `event Predicate<T>`
  三行的「类内触发」一栏，写的全是 `h.Invoke(args)`

实测（2026-09-26，main `4ae522839`，自建 SDK）：

```z42
Action<string> a = (string s) => Console.WriteLine(s);
a("x");          // ✅ 正常
a.Invoke("x");   // ❌ 运行期崩：VCall: expected object, got FuncRef("Main__lambda_0")
```

**编译期零诊断，运行期不可 `catch`**（`try { a.Invoke("x"); } catch (Exception e)` 接不住，
程序当场终止）。局部变量 / 普通字段 / `event` 字段**三种载体全崩**，arity 1 与 2 都崩。

⇒ 照着参考手册写单播 `event` 的触发模板，**必然崩**。

### 为什么活到今天：文档教的写法零测试覆盖

全仓 26 处 `.Invoke(` **全是反射的 `MethodInfo` / `ConstructorInfo`**（真对象，工作正常）；
stdlib 自己触发 handler 用的是**括号调用**——`MulticastAction.z42:122` `snapStrong[i](arg)`、
`:130` `w.Get()(arg)`。**没有任何测试在委托值上调过 `.Invoke()`。**

文档之所以看起来成立，是因为它把两种 `Invoke` 混在同一张表里：`:267-269` 多播那三行的
`X.Invoke(args)` 是 `MulticastAction<T>` 的**真方法**（`z42.core` 真类），确实能跑；紧挨着的
单播三行是**委托值**，跑不了。

### 这不是「实现漏了」，是刻意延后 + 文档超发

`docs/spec/archive/2026-05-02-add-delegate-type/proposal.md`：

- `:44` —— 「`d.Invoke(args)` 显式方法语法**留后续视需要补**」
- `:86`（Out of Scope）—— 「❌ `delegate.Invoke()` 显式方法语法（**v1 仅支持 `d(args)` 调用**）」

⇒ 当年明确划出了 scope。**真正的缺陷是参考手册在此之后承诺了「等价」**，没人把两边对上。
本变更按那句「留后续视需要补」把它补上——现在正是有需要（学习手册第 19 章要教委托，
而 `.Invoke` 是 C# 用户最熟的写法，`delegates-events.md §1` 目标 1 就是「C# 用户零迁移成本」）。

## What

让 `.Invoke(args)` 在**委托 / 函数类型值**上成为 `(args)` 的同义写法，脱糖到**同一个**
`BoundIndirectCall` 节点。

顺带把同一条路上两个「零诊断 + 运行期崩」补成编译期诊断（详见 design.md 的 D2 / D3）：

| 形态 | 今天 | 本变更后 |
|---|---|---|
| `f.Invoke(x)` | 零诊断 → 不可 catch 崩 | ✅ 等价于 `f(x)` |
| `f.Bogus()`（委托值上不存在的成员） | 零诊断 → 不可 catch 崩 | **E0401** |
| `f(1, 2)`（形参 1 个，多传） | 🔴 **静默丢掉多余实参**（lambda 照常收到 `x=1`） | **E1006** |
| `f()`（形参 1 个，少传） | 🔴 零诊断，形参拿到 `Null`，崩在**别的地方** | **E1005** |

后两条是本轮顺带实测出来的既存缺口：`BoundIndirectCall` 那条路**从不校验 arity**
（实参**类型**是查的——`f("str")` 正常报 E0402，所以缺的只有个数）。
`.Invoke` 若只脱糖不补这两条，等于把新写法接到一条已知漏水的管子上。

## Out of Scope

- **`f.Invoke` 不带括号**（当方法组取引用 / 属性读）—— 报诊断，不支持。
- **`BeginInvoke` / `EndInvoke`** —— `delegates-events.md §10` 已明确不做。
- **`DynamicInvoke`** —— 同上，与反射轨道一并设计。
- **多播 `MulticastAction<T>.Invoke`** —— 它是真类的真方法，走 class 分支，
  根本到不了本变更新增的 `Z42FuncType` 分支。**行为一字不变。**
- **`?.Invoke()`** —— `?.` 已随 `remove-null-conditional` 移除（E0480），不恢复。

## 类型与代价

**类型：`lang`**（新增语法形态 ⇒ 按 workflow 词汇警报走完整流程 1–9）。

- **零 VM 改动**：`.Invoke` 脱糖后发的就是括号调用今天发的 `CallIndirect`
  （`ExprEmitter.z42:180`），interp（`exec_call.rs:338`）与 JIT
  （`jit/helpers/closure.rs:125`）两侧都已支持。
- **零格式 bump**：不新增 IR 指令、不动 zbc / zpkg。
- **零指纹 bump**：不改变任何**已能编译**程序的产物字节（新增的只有「原本崩/原本无诊断」
  两档）。⚠️ 此条须在阶段 4 用摸底实证，不能想当然。

## 先例与取证

- **delegate 与 `(T) -> R` 在编译器里是同一个类型对象** `Z42FuncType`
  （`Z42Type.z42:476`；`StubCollector.z42:37` 注释「委托类型即函数类型」）⇒ 一条分支同时
  覆盖两种拼写。实测双向可互赋（`(int)->int` ↔ `Func<int,int>`），根因是
  `Z42FuncType.IsAssignableTo`（`Z42Type.z42:554-575`）走**结构比较**、不看名字。
- **已经有一个 `<FQ>.Invoke` 死体桩**（`StubEmitter.z42:186-219`，挂载
  `IrGenAuxEmitter.z42:87`），抬头注释写着「真实调用走 CallIndirect；供反射签名 + 重建」。
  它**不是**本变更的载体：VCall 按接收者运行期类型的 TypeDesc 解析，而 `FuncRef` /
  `Closure` 没有 TypeDesc（`vcall_resolve.rs:263` bail），永远派发不到它。
  ⇒ 本变更**不碰这个桩**，它继续只服务反射/元数据。
- **零诊断的根因**与坑点 ①（#801）同一条路：委托接收者在
  `_bindInstanceMemberCall`（`MemberResolver.z42:87`）**没有任何分支命中**，fallthrough
  到 prim 路径（`:311`）→ `HasClass("Action<string>")` 落空 → 跳过唯一的诊断闸门
  `BindPrimNoApplicableOverload` → 撞上「查无则松绑 Unknown」兜底
  （`MemberResolver.Prim.z42:43-44`，签名传 `null` ⇒ **连实参都不查**）。
  ⭐ #801 那次收窄只覆盖「包装类存在」的情形，**委托接收者连包装类查找都进不去**，
  所以那条收窄对它无效。本变更补的正是这一格。
