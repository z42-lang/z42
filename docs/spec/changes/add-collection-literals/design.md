# Design: 集合字面量脱糖实现原理

> 配套 [proposal.md](proposal.md)。记录**为什么这样实现**，使接手者不必通读源码即可理解脱糖架构。

## 总览：脱糖发生在语义层，产出既有构造

集合字面量是**纯前端语法糖**——parser 产出 5 个新 AST 节点，语义层（ExprTyper）把它们
**脱糖成现有的 Bound 构造**再交给既有 emitter，**不新增任何 IR / zbc / zpkg 格式**。

```
源码 [1,2,3] / {1,2,3} / {k:v} / [v;n] / [..a,..b]
  │  parser（ExprParser._parseArrayLit / _parseBraceLit）
  ▼
AST: ArrayLitExpr / ListLitExpr / DictLitExpr / ArrayRepeatExpr / EmptyBraceLitExpr
  │  ExprTyper._bindCollLit（脱糖核心）
  ▼
Bound: BoundArrayLit（数组无 spread，直接）
       BoundSeqExpr（其余：前置语句序列 + 值）  ← 唯一新增的 Bound 原语
  │  ExprEmitter.Emit
  ▼
既有 IR（array.new / call / vcall / 循环块）
```

## 三个关键设计决策

### D1. 为什么引入 `BoundSeqExpr`（序列表达式）

List/Dict/repeat/spread 的语义是「**先执行若干语句（new + 多次 Add/Set / 填充循环），再取某个
临时变量的值**」——这是一个 **block-expression**，而 z42c 原先没有。故新增 `BoundSeqExpr`：

```
BoundSeqExpr { BoundStmt[] Prelude; BoundExpr Value; }
```

发射（`ExprEmitter`）：依次发射 Prelude 各语句，再发射 Value 返回其寄存器。这是本 change **唯一
新增的 Bound 节点**——数组无-spread 直接复用既有 `BoundArrayLit`（等同 `new T[]{...}`）。

### D2. 为什么「合成 AST 再走既有绑定」而非手搓 Bound 节点

List `{a,b,c}` 要脱糖成 `tmp=new List<T>(); tmp.Add(a); tmp.Add(b); tmp.Add(c); tmp`。其中
`tmp.Add(x)` 是**实例方法调用**——其 emit 路径（DepIndex / VCall / 虚方法判定，见
`ExprEmitter._emitCall`）相当复杂，手工构造 `BoundCall` 极易把派发元数据填错。

因此脱糖采用**合成 AST**：在 ExprTyper 里构造 `ObjNewExpr` / `CallExpr(MemberExpr(...))` /
`WhileStmt` 等 AST 节点，再经**既有绑定器**（`_bindExpr` / `StmtBinder.BindStmt`）绑定——
所有类型解析、重载、方法派发元数据由既有路径自然填好。此手法在
[`BenchmarkDesugar.z42`](../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42) 已有先例。

合成的临时局部用 `$c0` / `$c1` … 命名（`$` 非法源标识符 → 与用户变量零冲突；作为字符串键在
Locals/env 映射中正常工作）。绑定合成 `VarDeclStmt` 时绑定器自身 `env.Define` 临时名，后续引用
自然解析。

### D3. emitter 侧的反向引用

`BoundSeqExpr` 的 Prelude 可能含 `while` 循环（repeat/spread 的填充）——发射循环需要
`FunctionEmitter._emitStmt`（含基本块 / 分支 / 循环上下文）。而 `ExprEmitter` 原先只持
`EmitContext`。故给 `ExprEmitter` 加一个 `FunctionEmitter` 反向引用，`BoundSeqExpr` 的 prelude
逐句委托 `_fn.EmitStmt(...)`。

> 表达式中途发射循环（新建基本块）是安全的——`BoundSwitchExpr` / 条件表达式早已在表达式中途
> 发射分支；整体控制流仍线性（循环完整内嵌），寄存器是函数级平表，循环后续指令进入 loop-end 块。

## 目标类型（target-typed）的传递

`_bindExpr` 不带 expected-type 参数（既有架构）。集合字面量的目标类型在**绑定站点**特判后，
以 **TypeExpr**（声明侧 AST 类型）显式传入 `_bindCollLit(expr, target, env)`：

- 站点：`StmtBinder._bindVarDecl`（`List<long> x = {..}` / 空 `[]`·`{}`）。`var` → 不传（走推断）。
- 用 TypeExpr 而非 Z42Type：有目标时可**直接复用声明的 AST 类型**作 `new` 的类型，免去
  `Z42Type → TypeExpr` 合成的摩擦。
- 无目标（`var xs = {1,2,3}` / 作实参）：`_bindExpr` 分派到 `_bindCollLit(e, null, env)`，元素类型
  由首元素/首键值推断，再经 `_typeToTypeExpr` 合成 `new` 的类型实参。

本轮**只**在 var-decl 站点接目标类型（proposal D3 边界）；实参按形参反推、泛型实参推断留后续。

## 脱糖对照表

| 字面量 | 脱糖 |
|--------|------|
| `[1,2,3]` | `BoundArrayLit`（直接，无 seq） |
| `[v; n]` | seq: `$v=v; $c=n; $a=new T[$c]; $i=0; while($i<$c){$a[$i]=$v;$i++}; ⟨$a⟩` |
| `[..a, x, ..b]` | seq: 各 spread 源 hoist → `$r=new T[Σ.Length] `→ `$k=0` → 逐段拷贝循环 → `⟨$r⟩` |
| `{1,2,3}` | seq: `$c=new List<T>(); $c.Add(1); $c.Add(2); $c.Add(3); ⟨$c⟩` |
| `{k:v}` | seq: `$c=new Dictionary<K,V>(); $c.Set(k,v); ⟨$c⟩` |
| `[]`（目标 `T[]`） | `BoundArrayLit` 空 |
| `{}`（目标 List/Dict） | `new List<T>()` / `new Dictionary<K,V>()` |

## 改动文件

| 文件 | 改动 |
|------|------|
| `z42c.syntax/src/Ast.z42` | +5 AST 节点（ArrayLitExpr / ArrayRepeatExpr / ListLitExpr / DictLitExpr / EmptyBraceLitExpr） |
| `z42c.syntax/src/ExprParser.z42` | 前缀 `[` → `_parseArrayLit`；表达式位 `{` → `_parseBraceLit` |
| `z42c.semantics/src/Bound.z42` | +`BoundSeqExpr` |
| `z42c.semantics/src/ExprTyper.z42` | `_bindCollLit` + 6 子绑定器 + `_typeToTypeExpr` + `_freshColl` |
| `z42c.semantics/src/StmtBinder.z42` | `_bindVarDecl` 集合字面量目标类型 hook；`public BindStmt` 包装 |
| `z42c.semantics/src/ExprEmitter.z42` | `BoundSeqExpr` 发射 + `FunctionEmitter` 反向引用 |
| `z42c.semantics/src/FunctionEmitter.z42` | `public EmitStmt` + 传 `this` 给 ExprEmitter |

## 已知边界（本轮 Out of Scope）

- spread 源仅数组（List 源用 `.Count` 留后续）。
- 无目标时非平凡元素类型（需 FQ 的用户类）合成可能失败 → 建议显式目标类型。
- 对象初始化器 `new Foo{X=1}` / 字段简写 / `..base`：后续 change。
- 实参按形参反推目标类型、泛型实参推断集合字面量。
