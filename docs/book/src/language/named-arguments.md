# 命名实参

> **页型**: 语言参考 ｜ **状态**: ✅ 已实现 ｜ **代码**: `z42c.syntax/ExprParser._parseCallArg` + `z42c.semantics/OverloadBinder._adaptArgs` + `z42c.semantics/CallParams`
> ｜ **对齐**: 2026-09-15（change `fix-crosspkg-named-args`；前序 `fix-overload-defaults-named-args` / `restore-named-arguments`）

任何形参都可以按**名字**传：

```z42
Painter.Draw("blue", filled: true);                  // 位置在前、命名在后
Painter.Draw(filled: true, color: "green", width: 3); // 全命名，可乱序
Painter.Draw("yellow", filled: true);                // 命名跳过中间的可选参
Greet(prefix: "Hi", name: "Alice");                  // 自由函数
var b = new Box(height: 30, width: 50);              // 构造函数
```

覆盖**全部调用形态**：自由函数、实例方法、静态方法、构造函数（含对象初始化器
`new P(y: 2) { Z = 3 }`），**同包与跨包一致**，也可以用在带 `params` 尾参的方法上。

## 规则

| 规则 | 说明 |
|---|---|
| 位置实参必须在命名实参**之前** | 命名之后再出现位置实参 → 该实参落到下一个空位，通常报错 |
| 命名实参之间**可乱序** | 按形参名归位，不按书写顺序 |
| 可跳过中间的**可选**形参 | 未被命名也未被位置填充的形参用其默认值 |
| 同一形参**不可重复** | 位置 + 命名同时命中一个形参 → 不可适配 |
| 名字必须匹配某个形参 | 否则该实参退回按表达式解析，通常报 `undefined: <name>` |

## 与重载、默认值一起用

有多个重载时，命名实参与省略的默认实参都参与**选哪个重载**（对标 C#）：

```z42
class M {
    public static string F() { .. }
    public static string F(string a, int n = 2) { .. }
    public static string K(int x) { .. }
    public static string K(string s) { .. }
}
M.F("a");            // 选 F(string, int = 2)：少给的形参有默认值
M.F(n: 7, a: "b");   // 选 F(string, int)
M.K(s: "x");         // 按名字选 K(string)
```

- 一个重载**适用**：每个实参都能落到一个形参上（位置依次、命名按名），类型可赋值；没有实参的形参都有默认值或是 `params`。
- 多个都适用时，先比较各实参对应的形参哪个**更具体**；仍平手时，**不需要补默认值**的重载优先，**不展开 `params`** 的形态优先。
- 仍分不出来 ⇒ E0425（歧义），加显式转换或换成命名实参。

```z42
class H { public static string G(int a) { .. } public static string G(int a, int b = 9) { .. } }
H.G(1);   // 选 G(int)：不需要默认值
```

构造器遵守同一套规则（`new C("a")`、`new C(n: 7, a: "b")`）。

## 与 `params` 一起用

```z42
static string P(string head, params int[] xs) { .. }
P(head: "h");                          // xs = 空数组
P(xs: new int[] { 1, 2 }, head: "h");  // 尾参按名字传数组，可乱序
P("h", 1, 2, 3);                       // 位置展开，照旧
```

- 命名实参时 `params` 尾参只能**整体**给（按名字传一个数组）或**省略**（得到空数组）。
- 「命名实参 + 展开的多个位置元素」（`P(head: "h", 1, 2)`）不支持——那几个位置实参没有空位可落，
  报「找不到方法」。需要展开就全用位置实参。

> 🔴 此前（`fix-crosspkg-named-args` 之前）**只要方法带 `params` 形参，任何命名实参调用都编不过**：
> 唯一候选是 params 方法时不走实参映射，命名实参的延迟占位被当成 target-typed `new`
> （`E0437` + `undefined: head`）；归位时又把「params 尾参没给」当成缺实参。

## 跨包

对另一个包里的函数 / 方法 / 构造器用命名实参，与同包完全一样：

```z42
using Demo.NaTarget;
Label("a", pad: "-");                   // 导入的自由函数
new Painter(size: 5, name: "pen");      // 导入的构造器
Painter.K(s: "x");                      // 同 arity 重载，按名字选
```

**形参名是包 API 的一部分**：改一个 `public` 方法的形参名，会让别的包里按旧名写的命名实参编不过
（对标 C#）。

### 机制

形参名一直在 zpkg 里——SIGS 段每个形参都有 `name_str_idx`（zbc 1.25 起恒写）。缺的是读包那一侧：

| 环节 | 此前 | 现在 |
|---|---|---|
| `TsigReconcile._params`（读包时从 SIGS 重建导出签名） | 名一律合成 `p0/p1/…`（沿用已删除的 TSIG 段的 C# 字节口径） | 取 SIGS 的形参源名（缺失才回落 `p{i}`） |
| `ImportedSymbolLoader._fillParamMeta` | 只填默认值 / caller 宏 | 同批填 `Z42FuncType.ParamNames` |
| `CallParams`（名字 → 第几个形参） | 不存在；`_adaptArgs` 与 `OverloadResolver.Map` 各写一遍、都只认本地 `MethodDecl` | 唯一出处：本地看 `MethodDecl`，导入看 `ParamNames` |
| `_adaptArgs` 补缺位 | 只认本地默认值表达式 | 导入缺位走 `_crossPkgDefault`（与位置调用的跨包补位同一条）；`params` 尾位补空数组 |

名字只在读包时重建进内存里的签名，**不写入任何新字节**，零格式变化。

同一 change 修掉的相邻缺口：**导入的自由函数此前拿不到默认值**——参数默认值以 `$Default` 哨兵挂在
`IrFunction.ParamAttrs` 上，而 `ParamAttrs` 只在类成员的发射路径（`IrGenMemberEmitter`）填，自由函数
（`IrGenAuxEmitter`）从不填 ⇒ 跨包 `Label("a")` 对 `Label(string text, int width = 8, …)` 报 `E1005`。

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
`f(x: new())`）一直在等一个永远不会到来的形态，而当时仓库根的演示文件整个用的都是这个语法、
**从来没有被编译过**。

⇒ 现在由行为 golden `src/tests/named-args/` 把关（断言**重排真的发生**，不是「能编过」就算数）。
