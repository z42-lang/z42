# Tasks: restore-named-arguments

> 状态：🟢 已完成 | 创建：2026-09-13 | 类型：lang（补回自举迁移时丢失的 parser 半边）+ fix

**变更说明：** 命名实参 `name: value` 在**所有调用形态**上恢复可用。这不是新特性——
原 spec（`add-named-arguments`, 2026-05-12）已经完整实现过，但**只实现在 C# bootstrap 编译器里**
（`z42.Syntax/Parser/ExprParser.Atoms.cs` 的 `IDENT :` 前瞻）。C# 编译器 2026-06-26 移除后，
**parser 这一半没有被移植到自举编译器**。

## 现场：一个等了三个月的语义层

- 语义层的归位逻辑 `OverloadBinder._adaptArgs` **一直在**（按形参名匹配、乱序、去重、
  目标类型绑定、类型检查、可选参填充），其注释里写的正是 `f(x: new())`。
- 但自举 parser 里没有 `IDENT :` 分支 ⇒ 那个形态**永远不会到达**语义层。
- 它唯一"能用"的地方是**构造函数**——因为 `_bindNew` 把 raw args 直接交给 `_adaptArgs`，
  而恰好 `AssignExpr` 是 `_adaptArgs` 认的载体，于是 `new P(y = 2)`（等号）碰巧可用。
- `examples/named_args.z42` 整个文件用的都是 `name:` 语法、**从来没有被编译过**
  （顶层 `examples/` 无人编译，见 `gate-toplevel-examples`）。

⭐ 这条正是 memory `selfhost-migration-lost-negative-tests` 记的「命名实参（parser 无 Colon 分支）」。

## 改动（三处 + 一个既有 bug）

1. **parser**：`ExprParser._parseCallArg()` —— `IDENT :` 前瞻，产出语义层已认的载体
   `AssignExpr("=", Ident, value)`（**不新增 AST 节点**）。调用与 `new` 共用同一个 helper。
   🔴 不与三元 `a ? b : c` 相撞：那里的 `:` 在内层表达式**之后**，而本前瞻要求 `:` 紧跟在
   实参**开头**的标识符后。已实测：三元实参 / Dict 字面量 `{ "a": 1 }` / `switch case 1:` 全不受影响。
2. **`_withDefaults`**：命名实参分支**提到所有绑定步骤之前**。此前第一步 `BindArgsToSignature`
   会把延迟槽**按位置**重新绑 ⇒ 命名实参的槽位还没定就被当普通表达式绑掉，`y` 立刻 `undefined`。
3. **`_bindCall`**：命名实参与 target-typed `new` / lambda 同款延迟（留 `null`）。
4. 🔴 **既有 bug（种子编译器同样有，非本 change 引入）**：`_adaptArgs` 把**任何**
   `AssignExpr(Ident, …)` 都当命名实参，**不看那个标识符是不是作用域内变量** ⇒ 真赋值实参
   `Greet(who = "Bob")` 被误判 → 适配失败 → **默认参数填充被跳过**，可选形参静默留 `null`
   （实测打印 `null, Bob` 而非 `Hello, Bob`）。判据统一到 `ExprTyper.IsNamedArg`。

## 判据：`f(x = 1)` 是命名实参还是赋值实参

**看 `x` 是不是当前作用域里的变量**：是 → 真赋值（赋值后传值）；不是 → 命名实参。
这条保守规则只把「否则本来就会绑失败」的形态改判 ⇒ **对既有代码零行为变化**。
实测语料：全仓 `f(x = v)` 形态命中 24 处，**全是 attribute 实参**（`[Native(lib = "…")]`，
另一条解析路径），方法调用位**一处都没有**。

## 验证

- [x] 六种调用形态全通：自由函数 / 实例方法 / 静态方法 / 构造函数 / 对象初始化器 / 跳过中间可选参
- [x] 语法不相撞：三元实参、Dict 字面量、`switch case`
- [x] 负例仍报错：错名、重复命名、类型不符（`cannot assign string to Int32 (argument)`）
- [x] `examples/named_args.z42` **编过并跑对**（乱序命名与构造函数重排都验了输出）
      ⇒ 已从 `examples-known-broken.txt` 移除（**双向棘轮如设计般要求删这一行**）
- [x] 行为 golden `src/tests/named-args/`：断言**重排真的发生**（不是「能编过」就算数）
- [x] `xtask test` **13 个 stage 全部通过**（`✅ GREEN — all stages passed`）+ **自举不动点 3/3**
      + 顶层 examples 门 **18 编过 / 4 已知欠债 / 0 失败**
- [x] ⚠️ 但 `xtask test` **退出码是 1**：崩在**所有 stage 通过之后**的耗时表打印
      （`_stageSummary`: `type mismatch in arithmetic: Char('z') vs I64(100)`）。
      **归属已用硬证据定死：不是本 change 引起的** —— 把 `xtask_test.z42` 在「基线编译器」与
      「本 change 编译器」下各 dump 一次 IR，**1651 行逐行完全一致**。且它**数据相关**：
      同为 13 stage，前一个 worktree（z42-gates）正常打印过这张表，本轮 `stdlib [Benchmark]`
      跑了 5m45s、时长分布不同才触发。已单独记为发现，未在本 change 内修。
- [x] 零格式 bump

## ⚠️ 本轮踩到的两个环境坑（都不是回归）

1. **改 stdlib 后必须重建 `artifacts/xtask/xtask.zpkg`**：`z42c.syntax` 属 `src/libraries/`，
   stdlib 被重建而种子建的 xtask.zpkg 没有 ⇒ GREEN 所有 stage 都过、却崩在 xtask 自己打印
   耗时表（`type mismatch in arithmetic: Char('z') vs I64(100)`）。它**不按 mtime 自动重建**。
2. **`z42.net` 的 `http_server_threaded` 是已知并发 flake**（`concurrency-null-thread-flake`）：
   这一轮先是 `Std.ThreadException: … got Null`，重跑变成**挂住**。修它的 #617 尚未合并。
