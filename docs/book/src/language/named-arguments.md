# 命名实参

> **页型**: 语言参考 ｜ **状态**: ✅ 已实现 ｜ **代码**: `z42c.syntax/ExprParser._parseCallArg` + `z42c.semantics/OverloadBinder._adaptArgs`
> ｜ **对齐**: 2026-09-13（change `restore-named-arguments`）

任何形参都可以按**名字**传：

```z42
Painter.Draw("blue", filled: true);                  // 位置在前、命名在后
Painter.Draw(filled: true, color: "green", width: 3); // 全命名，可乱序
Painter.Draw("yellow", filled: true);                // 命名跳过中间的可选参
Greet(prefix: "Hi", name: "Alice");                  // 自由函数
var b = new Box(height: 30, width: 50);              // 构造函数
```

覆盖**全部调用形态**：自由函数、实例方法、静态方法、构造函数（含对象初始化器
`new P(y: 2) { Z = 3 }`）。

## 规则

| 规则 | 说明 |
|---|---|
| 位置实参必须在命名实参**之前** | 命名之后再出现位置实参 → 该实参落到下一个空位，通常报错 |
| 命名实参之间**可乱序** | 按形参名归位，不按书写顺序 |
| 可跳过中间的**可选**形参 | 未被命名也未被位置填充的形参用其默认值 |
| 同一形参**不可重复** | 位置 + 命名同时命中一个形参 → 不可适配 |
| 名字必须匹配某个形参 | 否则该实参退回按表达式解析，通常报 `undefined: <name>` |

## 与赋值实参的区分

`f(x = 1)` 有歧义：既可能是「名为 `x` 的命名实参」，也可能是「把 1 赋给变量 `x` 再传值」。
z42 的判据是 **`x` 是不是当前作用域里的变量**：

- **是变量** → 真赋值表达式（赋值后传值），与命名实参无关。
- **不是变量** → 当作命名实参（与 `x: 1` 等价）。

判据只此一份（`ExprTyper.IsNamedArg`），解析期的延迟决定与绑定期的归位共用它。

> 🔴 **这里曾有一个静默错值的 bug**（本 change 一并修）：判据以前**不看是不是变量**，于是
> 一个货真价实的赋值实参 `Greet(who = "Bob")` 被误判成「名为 `who` 的命名实参」→ 没有这个
> 形参 → 整个适配失败 → **默认参数填充被跳过**，可选形参静默留 `null`（实测打印
> `null, Bob` 而非 `Hello, Bob`）。

## 历史：这个特性丢过一次

原 spec（`add-named-arguments`, 2026-05-12）是在 **C# bootstrap 编译器**里实现的
（`z42.Syntax/Parser/ExprParser.Atoms.cs` 的 `IDENT :` 前瞻）。C# 编译器 2026-06-26 移除后，
**parser 这一半没有被移植到自举编译器**——语义层的归位逻辑（`_adaptArgs`，其注释里写的正是
`f(x: new())`）一直在等一个永远不会到来的形态，而 `examples/named_args.z42`
整个文件用的都是这个语法、**从来没有被编译过**（顶层 `examples/` 无人编译，见
change `gate-toplevel-examples`）。

⇒ 两道门现在同时盯着它：顶层 examples 编译门（`named_args.z42` 已从已知欠债名单移除）
+ 行为 golden `src/tests/named-args/`（断言**重排真的发生**，不是「能编过」就算数）。
