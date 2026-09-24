# Proposal: `switch` 表达式无匹配臂时抛异常，不再静默产垃圾值

## Why

`switch` **表达式**在所有臂都不匹配时，**读到一个从没被写过的寄存器**。不是崩溃、不是
类型默认值，而是**垃圾值**。

实测（2026-09-22，origin/main `2dd01a9a9`，interp）：

```z42
int n = 5;
int    a = n switch { 1 => 10, 2 => 20 };   // a 打成 null，a+1 打成 1
bool   b = n switch { 1 => true };          // b 打成 null
string s = n switch { 1 => "x" };           // s 打成 null
```

`a` 声明为 `int` 却打印 `null`，`a + 1` 打印 `1`；换个程序形状同一条路径曾打出
**17179869186**（记忆 `z42-switch-expr-no-match-null`）。垃圾值随寄存器分配漂移，
**错值会一路流到很远的地方才以别的形态崩**。

### 为什么诊断顶不住这件事

`W0700`（switch 不穷尽）**只查封闭域**——`bool` 与 `enum`
（`ExhaustCheck.z42:5-10`，`sealed` 已在该 change 里判为 out-of-scope）。

⇒ 上面三个复现里 subject 都是 `int`，**开放域，一条诊断都没有**。
「把 W0700 升级成 error」治不了这一类，而这一类恰是最危险的那类（数值垃圾值）。

C# 在这里抛 `SwitchExpressionException`。

### 根因

`OperatorEmitter._emitSwitchExpr`（`:149-186`）：

```z42
TypedReg result = this._ctx.Alloc(this._ctx.ToIrType(sw.Type()));
... 逐臂：匹配 → 写 result → Br(endL)；失败 → nextL ...
if (!this._ctx.Ended) { this._ctx.EndBlock(new BrTerm(endL)); }   // ← 全部落空走这里
this._ctx.StartBlock(endL);
return result;                                                     // result 从没被写过
```

没有兜底臂时，最后一个 `nextL` 块直接 `Br(endL)`，`result` 一次都没写。
有 `default` / `_ =>` 臂时这条路不可达（那一支 `ai = sw.ArmCount` 提前结束），
所以**只影响不穷尽的 switch 表达式**。

## What Changes

### 语义（User 已裁决：抛，对齐 C#）

`switch` **表达式**求值时若无任何臂匹配（含守卫为假），抛
**`Std.SwitchExpressionException`**（新增类，User 已裁决用专用类而非复用
`InvalidOperationException`）。

`switch` **语句**不变——语句形态不产值，无匹配就什么都不做（C# 同）。

### 落点：纯 emitter，零格式变更

落空点由「`Br(endL)`」改为发射：

```
ConstStr  msg0 = "switch expression did not match any arm; value: "
ToStr     vs   = <subject>                    // 复用插值那条三分支（enum 装箱 / struct ToString / 其余 ToStr）
StrConcat msg  = msg0 ++ vs
ObjNew    exc  = Std.SwitchExpressionException(msg)
Throw     exc
```

全部用**现有指令**（`ConstStrInstr` / `ToStrInstr` / `StrConcatInstr` / `ObjNewInstr` /
`ThrowTerm`）。**不新增 opcode、不动 zbc·zpkg wire 格式、VM 一行不改。**

> **为什么不新增一条「抛内置异常」指令**：那会新增 opcode ⇒ zbc minor bump ⇒ 连带 zpkg
> minor bump + `version-bumping.md` 9 步 + 6 个 zbc / 4 个 zpkg 字节基线重生 + 触发
> ci-bootstrap 两代自举。而 `ObjNewInstr` 的类名与 ctor 名本就是**裸字符串字段**
> （`IrInstrObject.z42:52-54`），emitter 直接合成即可，先例是 `RecordSynth.z42:232-262`
> （合成 ConstStr + 多块控制流）。

### 位置信息从栈回溯来，不进消息

运行期 `Terminator::Throw` 已做 `resolve_line` + `update_top_frame_pos` +
`populate_stack_trace`（`interp/mod.rs:336-345`；JIT 侧 `jit_throw`
`jit/helpers/control.rs:16-30` 同）。⇒ 抛出点的 `file:line` **自动出现在栈回溯里**。

消息里**刻意不嵌源码路径**：`Span.File` 在发射期是构建机上的路径，嵌进去会把构建目录
烤进 zbc ⇒ 破坏字节不动点与可复现构建。

### 新增 `Std.SwitchExpressionException`

`src/libraries/z42.core/src/Exceptions/SwitchExpressionException.z42`，与既有 18 个
异常类同形（`InvalidCastException.z42` 是最近的同形先例，#746）。

## 影响面（已实测，不是估计）

全仓扫「`switch` 表达式且无无条件兜底臂」的站点：

| 域 | switch 表达式 | 其中无兜底 | 说明 |
|---|---|---|---|
| `src/compiler` + `src/libraries` + `src/toolchain` + `scripts` | 17 | **0** | 13 个命中全在 `z42c.semantics/tests/exhaust/exhaust_tests.z42` 里当**测试输入的源码串**，从不执行 |
| `src/tests`（e2e golden） | 23 | 5 | `pattern_core` ×2 / `pattern_exhaust_sealed` ×2 / `pattern_generic` ×1 |
| `examples` | 8 | 4 | 含 `exhaust/missing.z42` 与 `exhaust/closed.z42` —— 这两个**现在就是拿落空当教材** |

三条推论：

1. **z42c / stdlib 自身发码不变**（产品代码零站点）⇒ 自举字节不动点不受影响、
   `xtask test bootstrap` 的越界检查不受影响。
   ⚠️ **但 `CacheStore.CompilerFingerprint` 仍必须 bump（8 → 9）**：它是**缓存失效 pin**
   （`CacheStore.z42:6`「codegen/优化/typecheck 变化即令旧条目作废」）。用户代码里含不穷尽
   switch 表达式的文件，其**源码哈希不变而发码变了** —— 不 bump 就会复用旧编译器产出的
   buggy 缓存条目。记忆 `z42-switch-expr-no-match-null` 把 bump 的理由记成「z42c 自身源码里
   有不穷尽 switch」，**那条理由是错的**（实测 0 个），但结论仍成立，换了根据。
2. **`ThrowTerm` 的两个优化代价在本仓实际为零**：
   `IrInline._termInlinable`（`:316`）白名单只有 `Ret|Br|BrCond` ⇒ 含 `ThrowTerm` 的函数
   不可内联；`IrPureFunctionTable._isFuncPure`（`:62`）遇 `ThrowTerm` 判非纯 ⇒ 丢 LICM /
   PureCall。产品代码零站点 ⇒ 不触发。**用户代码里会触发**，记进 reference。
3. `examples/types/patterns/exhaust/` 两例 + 第 17 章那一节要**改写**——它们当前把
   「静默产 null / 垃圾值」当规则写着（`patterns.md:52-75,204-205`）。

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/libraries/z42.core/src/Exceptions/SwitchExpressionException.z42` | NEW |
| `src/compiler/z42c.pipeline/src/CacheStore.z42` | `CompilerFingerprint` 8 → 9 + 原因注释 |
| `src/compiler/z42c.semantics/src/OperatorEmitter.z42` | `_emitSwitchExpr` 落空点发 new + throw |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | 抽出 `internal TypedReg _emitToStr(reg, type)`，插值与本变更共用（不复制那条三分支） |
| `src/tests/types/switch_expr_no_match/**` | NEW e2e：三种形态 + `catch` 抓得到 + 有兜底臂不抛（阴性对照） |
| `src/tests/pattern-matching/pattern_core.z42` 等 3 个 | 若落空路径真被执行则改；只声明不执行则不动 |
| `examples/types/patterns/exhaust/{missing,closed}.z42` + `run.console` | 改写成「示范异常」 |
| `docs/learn/src/types/patterns.md` | 改写穷尽性那一节 + 章末要点 |
| `docs/reference/src/language/pattern-matching.md` | switch 表达式的无匹配语义 |
| `docs/reference/src/stdlib/*`（异常清单页） | 新异常类 |
| `docs/reference/src/appendix/error-codes.md` | W0700 词条补「表达式形态运行期会抛」 |

## Out of Scope

- **`W0700` 的级别不动**（仍是 warning）。理由：它只覆盖封闭域，升级成 error 既治不了
  开放域那一类、又会把今天能编的 bool/enum switch 表达式变成编译错误（破坏性变更）。
  抛异常已经把「静默错值」这个真问题解决掉了。
- `switch` **语句**不动。
- `sealed` 层次的穷尽性判定不动（`add-internal-hierarchy-exhaustiveness` 那条线的事）。
- 不新增 opcode、不 bump zbc·zpkg 格式（刻意避开）。
- 消息里不带源码位置（理由见上；位置从栈回溯来）。

## Open Questions

### Q1 —— `ObjNew.CtorKnown` 对合成站点是否置得上？

`CtorKnownFixup.Apply`（`PackageCompile.z42:283`）在整包装配后遍历所有 `ObjNewInstr`，
按「本包已发射函数 ∪ DependencyIndex」判可见性（`CtorKnownFixup.z42:88`）。
`Std.SwitchExpressionException` 在 z42.core，用户包必然依赖它（prelude）⇒ 预期置得上。

**须实测**：位没置上时运行期不调 ctor、`Message` 为空 ⇒ 异常照抛但消息丢了。
tasks 里有一条专门验消息非空。

### Q2 —— 泛型 / struct subject 的 `ToStr`

`ToStrInstr` 对 blob struct 只吐 `<struct value>` 占位符
（`ExprEmitter.z42:303-306` 有完整注释）。抽出的 `_emitToStr` 原样复用插值那条三分支
（enum → `_emitEnumBox`；blob struct → `_emitStructToStr`；其余 → `ToStrInstr`），
所以行为与 `$"{x}"` 一致——**不引入新的占位符坑，也不额外修它**。
