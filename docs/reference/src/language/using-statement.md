# `using` 语句

> **页型**: 参考页 ｜ **代码**: `z42c.syntax/src/StmtParser.z42`（解析）· `z42c.semantics/src/StmtBinder.z42`（降糖与判定）｜ **对齐**: 2026-09-26

`using` 语句把「用完一定释放」写成一行。它与 `using` **指令**（命名空间导入、类型别名）
是同一个关键字的两种用法，靠 `using` 后随的 token 区分 —— 见下方[与 `using` 指令的区分](#与-using-指令的区分)。

## 四种形态

```z42
using (OpenFile("a.txt")) { ... }          // ① 只要作用域，不要名字
using (TextReader r = OpenFile("a.txt")) { ... }   // ② 块内可见的名字
using var r = OpenFile("a.txt");           // ③ 作用域到**所在块末尾**
using TextReader r = OpenFile("a.txt");    // ④ 同③，显式类型
```

①② 有自己的块；③④ 没有 —— 它们的作用域一直延伸到**所在块的末尾**，与 C# 的 `using var` 一致。

## 语义

任一退出路径都释放。**五条**都覆盖（由 `try`/`finally` 保证，与 `foreach` 的枚举器路径同一套）：

| 退出路径 | 释放？ |
|---|---|
| 正常落出块尾 | ✅ |
| `return` 穿出 | ✅ |
| `break` 穿出 | ✅ |
| `continue` 穿出 | ✅ |
| 抛异常穿出 | ✅（原异常继续传播，不被释放动作盖掉）|

`null` 目标**跳过**释放（与 C# 一致）：`using (MaybeNull()) { ... }` 不会空引用崩。
这一点很要紧 —— 若在释放处崩，它会盖掉块里真正的异常。

多个 ③④ 形态叠加时**逆序释放**（后获取的先释放）：

```z42
using var a = Open("a");
using var b = Open("b");
// 退出时：先 b.Dispose()，再 a.Dispose()
```

## 目标类型必须**名义实现** `Std.IDisposable`

```z42
class Res : IDisposable {          // ← 必须有这个基表
    public void Dispose() { ... }
}
```

**光有 `Dispose()` 方法不够**，报 `E0498`。这与 C# 一致，也与 `foreach` **刻意不同** ——
`foreach` 的枚举器走**鸭子类型**（有 `MoveNext`/`Current` 的形状就行，`Dispose` 有则调），
而 `using` 走名义。两者判据不同源是有意的裁决，不是漏改。

判定问的是**接口闭包**，所以这两种继承形态都成立：

```z42
class Base : IDisposable { public void Dispose() { } }
class Derived : Base { }                     // ✅ 经基类继承

interface IRes : IDisposable { }
class R : IRes { public void Dispose() { } }
IRes r = new R();                            // ✅ 经父接口继承
```

## 与 `using` 指令的区分

| 写法 | 是什么 |
|---|---|
| `using (` … | **语句** |
| `using var` … | **语句** |
| `using <类型> <标识符> = ` … | **语句**（形态 ④）|
| `using Std.Toml;` | **指令**（命名空间导入）|
| `using Alias = Std.Toml;` | **指令**（类型别名）|

形态 ④ 与类型别名的区别是 `=` 之前有**两个** token（类型 + 名字）而不是一个。
指令只能出现在文件顶部；写在方法体里报 `E0209`。

## 可以关掉

`z42.toml` 里 `[syntax] using_stmt = false` ⇒ 用 `using` **语句**报 `E0301`；
`using` **指令**不受影响（关掉一个语法特性不该改变一条无关诊断的理由）。

## 关联页面

- [迭代（foreach）](iteration.md) —— 枚举器路径的 `Dispose` 是条件步骤，且判据是鸭子类型
- [异常](exceptions.md) —— `try`/`finally` 的退出路径保证
- [语法总览](syntax.md) —— `using` 指令一节
- [委托与事件](delegates-events.md) —— 订阅 token 实现 `IDisposable`，可直接进 `using`
