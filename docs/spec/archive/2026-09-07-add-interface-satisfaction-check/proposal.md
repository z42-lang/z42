# Proposal: 接口成员齐备性校验 —— 声明了接口就必须真的实现它

> 类型：lang（新诊断）｜ 创建：2026-09-07
> 缘起：`apply-self-to-core-protocols`（#525）实施期实证发现

## Why

z42 **完全不校验**「一个类声明了接口，有没有真的实现它」。实测探针（改动前）：

```z42
interface I { void M(); }
class C : I { }                              // 缺 M —— 不报错
interface J { void N(Self s); }
class D : J { public void N(int x) { } }     // 签名与 Self 不符 —— 不报错
```

两条都静默通过。`InheritanceResolver.z42:14` 自陈「严格校验（接口方法齐备/arity）留待后续」，
而错误码 **`E0412 InterfaceMismatch` 早已声明、全仓零引用**——规范写好了，实现从来没落。

两份设计文档甚至已经把算法写死了：
[`static-abstract-interface.md` §4.2](../../../design/language/static-abstract-interface.md)
与 [`generics.md`](../../../design/language/generics.md) 都指名 E0412，并明写「签名必须匹配
（`T → C` 替换后）」。本 change 就是把这份既有规范实现出来。

**它为什么现在重要**：`Self` 落地（#506）+ stdlib 改写（#525）之后，「实现方签名必须与
`Self` 替换后的接口签名一致」成了**标准库正确性的支柱**——而这一条恰恰无人看守。#525 实施期
我明确记录过「本次受益于这个洞」：基元写 `Equals(int)` 而 `Self` = `Int32`，不被拒纯属没人查。

## What Changes

`InheritanceResolver` 新增 pass ⑥ `_checkIfaceMembersComplete`（与 ⑤ 关联类型绑定同一时机、
同一理由：要求接口成员已收集完），对类声明的每个接口逐成员校验，两类诊断都发 **E0412**：

| 情形 | 诊断 |
|---|---|
| 接口成员的**名字**在实现方（含基类链）完全不存在 | ``does not define member `M` `` |
| 名字在、但没有任何重载匹配签名 | ``no overload of `N` matches …(want `N$1$string`)`` |

### 三处必须做对，否则就是误报

1. **`Self` 替换成实现类**：`class Q : IEq` 期望 `Same(Q)`。基元同样成立——`Self := Int32`，
   而源码写的 `int` 与 `Int32` 经 `CanonName()` 同为 `i32`，故 12 个基元全部匹配得上。
2. **接口型参替换成实现子句给的实参**：`class Bag<U> : IColl<U>` 里接口的 `T` 绑到 `U`。
   ⚠️ **必须从 AST 的 `c.Bases` 重解析**——`ct.InterfaceNames` 只存裸名，类型实参在
   `StubCollector:167` 就被丢掉了，拿裸名比签名会把这种写法误报成 `AddOne$1$U` vs `AddOne$1$T`。
3. **按名取全部重载，不能按名索引**：`String` 有 `Equals(object?)` 与 `Equals(string)` 两个重载，
   前者占裸键、后者占 `Equals$1$string`（primary/非-primary 规则）。`Methods.Get("Equals")` 会
   拿到错的那个 → 假阳性。改用 `ct.OverloadsOf(name)` 并沿基类链上溯。

### 顺带修：`ImportedSymbolLoader` 的 `IsStatic` 被抹平

`ImportedSymbolLoader.z42:266` 构造导入接口成员时把 `isStatic` **硬编码 `false`**，而
`mz.IsStatic` 就在手上 ⇒ 跨包看 `INumber.op_Add` 这类 **static abstract** 成员会被当成实例方法。
与 #523 修的四条「imported 类型保真度」同族：元数据带着信息，导入侧把它抹平了。已改为读
`mz.IsStatic`。（`IsAbstract` 无对应槽——`MethodSymbol` 没有该字段，抽象性一律靠 `Decl.Mods`
嗅探而导入符号无 `Decl`；故只还原 `IsStatic`。）

## 🎯 欠债实测：**零**

User 裁决「先实现全集并量欠债」。开门前全仓实测（已清 `.cache` 避免 warm 低估）：

| 面 | E0412 违反数 |
|---|---|
| `build stdlib`（25 个库，含 String / 12 基元 / collections） | **0** |
| `build compiler`（z42c 全部子系统） | **0** |
| `xtask test compiler`（含既有 584+ 用例） | **0 失败** |

⇒ **无需任何清理，可以直接开门。**

### 「零违反」与「通道没通」的区分（两道对照，都已实测）

1. **阳性对照**：4 个刻意违反的用例全部报出，且消息精确——
   `Empty` 缺 M/N 各一条、`WrongSig` 报 ``want `N$1$string` ``、`Q` 报 ``want `Same$1$Q` ``
   （证明 `Self := Q` 替换生效）；`Ok` / `P` 两个正确类**不报**。
2. **真实构建面的破坏性对照**：把 `Boolean.Equals(bool)` 临时改成 `Equals(int)`，
   `build stdlib` 立即失败并报
   ``Boolean` implements `IEquatable` but no overload of `Equals` matches …(want `Equals$1$bool`)``。
   ⇒ 校验确实在**真实 stdlib 构建路径**上活着，而不是只在单测里。

## 不在本轮

- **static-vs-instance 种类校验**（「同名 static 但无 `override` → 隐藏接口成员」，设计文档
  `generics.md:658-661` 提到）：本轮只比名字与签名，不比 static/instance 种类。修好的
  `IsStatic` 保真度是它的前置，但那条判据本身要先想清楚 `MethodSymbol` 无 `IsAbstract` 槽怎么办。
- **`impl Trait for Target` 块补充的接口**：`_passImpls` 在 `_passSealedEnforce` **之后**跑，
  故经 impl 块获得的接口此刻还不在 `InterfaceNames` 里，本轮不查（不是回归——此前一条都不查）。

## 验证口径

完整 GREEN + `test stdlib --mode jit` + cross-zpkg jit + `test bootstrap` + 自举字节不动点。
