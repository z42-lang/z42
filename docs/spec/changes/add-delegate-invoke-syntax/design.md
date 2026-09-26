# Design: 委托值上的 `.Invoke(args)`

## D1 —— 载体：脱糖到括号调用**同一个**绑定节点

`_bindInstanceMemberCall`（`MemberResolver.z42:87`）是所有接收者的唯一漏斗，按接收者类型
逐个 `is` 分支（`:102` class / `:143` interface / `:192` error·unknown / `:199` instantiated /
`:251` 型参 / `:299` 数组），委托/函数类型**一个都不命中**，fallthrough 到 `:311` 的 prim 路径。

**在 `:299` 数组分支之后、`:311` fallthrough 之前插入 `rt is Z42FuncType` 分支**：

```
if (rt is Z42FuncType) {
    Z42FuncType ft = rt as Z42FuncType;
    if (mem.Name == "Invoke") {
        <arity 校验：见 D3>
        return new BoundIndirectCall(recv, args, argCount, ft.Ret, sp);   // 与括号调用同一个节点
    }
    <报 E0401：见 D2>
}
```

产出的 `BoundIndirectCall` 与 `MemberResolver.z42:666`（`arr[i](x)` 那条任意表达式路）
**完全同型**，因此：

- 发射端零改动 —— `ExprEmitter.z42:173-182` 已认这个节点，发 `CallIndirectInstr`（`:180`）
- VM 零改动 —— interp `exec_call.rs:338`、JIT `jit/helpers/closure.rs:125` 已支持
- 返回类型取 `ft.Ret`，与括号调用一致（此前走 prim 兜底拿到的是 `Z42UnknownType`）

⭐ **为什么不走那个已有的 `<FQ>.Invoke` 桩**（`StubEmitter.z42:186-219`）：它是 virtual 死体桩
（体 = `ret null`），只能经**有 TypeDesc 的对象**派发；`FuncRef` / `Closure` 没有 TypeDesc
（`vcall_resolve.rs:263` 直接 bail）。桩继续只服务反射签名与跨包元数据重建，**本变更不碰它**。

## D2 —— 委托值上的其它成员名：报 E0401（复用，不取号）

今天 `f.Bogus()` 零诊断、运行期崩 `VCall: expected object, got FuncRef`。
新分支里非 `Invoke` 的成员名一律报 **E0401**，与 prim 收者（#801）、型参收者（#833）
**同一条口径**——那两次收窄漏掉委托这一格，正是因为委托接收者连包装类查找都进不去
（`HasClass("Action<string>")` 恒 false ⇒ 跳过唯一的诊断闸门）。

措辞照抄现有家族：`no method \`Bogus\` on delegate type \`Action<string>\``。

⚠️ **不套用 prim 那条 stub 豁免**（「包装类是空表 stub 时仍松绑定」）：委托类型的成员面是
**语言固定的**（只有 `Invoke`），不存在「懒加载导致候选集不完整」的情形 ⇒ 无需豁免，
判「不存在」不会误报。

## D3 —— 顺带补 arity 校验（**本变更的范围决策点**）

实测（main `4ae522839`）`BoundIndirectCall` 那条路**从不校验实参个数**：

| 写法（形参 1 个） | 今天 |
|---|---|
| `f(1, 2)` | 🔴 **静默丢掉多余实参**，lambda 照常收到 `x=1`、返回 2，零诊断 |
| `f()` | 🔴 零诊断，形参拿到 `Null`，崩在**别处**（`type mismatch in arithmetic: Null vs I64(2)`）|
| `f("str")` | ✅ E0402（实参**类型**是查的，缺的只有个数）|

**建议：在三个 `BoundIndirectCall` 产出点统一校验 `argCount` vs `ft.ParamCount`**，
少报 **E1005**、多报 **E1006**（两码均已有发射点：`OverloadBinder.z42:287,290,292,294`，
**零取号**）。三个产出点：

- `MemberResolver.z42:588-594`（裸标识符 `a(x)`）
- `MemberResolver.z42:661-673`（任意表达式 `arr[i](x)`，判据在 `:666`）
- 本变更新增的 `.Invoke` 分支

⚠️ **`:605-609` 的型参分支（`where T : Func<..>`）不在内** —— 它的返回类型是
`Z42UnknownType`、形参表在擦除后不可信，校验会误报。保持现状。

### 为什么建议一起做

`.Invoke` 若只脱糖不补这两条，等于把一个新写法接到一条已知漏水的管子上，而且
**「多传被静默丢掉」是比崩溃更坏的形态**（错值一路流走）。一条检查同时治两种拼写。

### 🔴 实施纪律（这条最容易翻车）

**加检查类的改动，必须先看存量测试会不会红，再看自己的新测试绿不绿**（本仓已在
#833 上栽过一次：12 条新诊断单测全绿，却被存量 e2e 的惯用写法判红）。
⇒ 阶段 4 必须先全仓摸底：跑不带过滤的 `cargo test --lib` + `xtask test`，
统计有多少站点今天靠「多传被静默丢掉」活着。**若摸底命中非零，先把命中报给 User 再决定
是否收窄**，不得自行放宽判据。

## D4 —— 不受影响的面（须有阴性对照守住）

| 面 | 为什么不受影响 |
|---|---|
| 多播 `MulticastAction<T>.Invoke(arg)` | `MulticastAction<T>` 是 `z42.core` 真类 ⇒ 走 `:102` class 分支，到不了新分支 |
| 反射 `MethodInfo.Invoke` / `ConstructorInfo.Invoke` | 同上，真类真方法（全仓 26 处 `.Invoke(` 全属此类）|
| 括号调用 `f(args)` | 三条路的判据与产出节点不变，只加 arity 校验 |
| `<FQ>.Invoke` 死体桩与反射签名 | 本变更不碰 `StubEmitter` / `IrGenAuxEmitter` |

⚠️ 前两行必须各有一条 golden/e2e 阴性对照 —— 「委托的 Invoke 通了」与「真类的 Invoke 没被
劫持」是两件事。另：定位调查曾提出一条假说「`CallEmitter.z42:243` 的 DepIndex instance 捷径
可能把 `Invoke/arity-N` 劫持到 `MulticastAction.Invoke`，发出**直接 Call 到错函数**」——
实测 arity 1 与 2 **均未复现**（都是 VCall 崩）。本变更后该路径整条不再被走到，但
**阴性对照仍要留**，因为它是「静默调错函数」这一档最坏形态的唯一守门。

## 风险与代价

- **格式**：零。不新增 IR 指令，不动 zbc / zpkg。
- **指纹**：预期零 bump（新增的只有「原本崩」与「原本无诊断」两档，不改变任何已能编译
  程序的产物字节）。⚠️ **须在阶段 4 用摸底实证**——`fingerprint` 门不在 `xtask test` 默认档里
  （独立子命令且要 `--base` 参考树，只有 CI 跑）⇒ 这类判断必须手动做，不能指望门禁提醒。
- **文档**：`delegates-events.md:61,62,264-266` 四处从「谎报」变成「准确」；
  `closures.md` 的调用相关段落复核一遍；学习手册第 19 章据此落地。
- **回滚**：删掉新增分支即回到今天行为（脱糖是纯加法，无既有代码路径被改写）。
