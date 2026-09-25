# Tasks: 装箱点收到 Null 时响一声（debug）

> 状态：🟢 已完成 | 创建：2026-09-25 | 完成：2026-09-25
> 分支/worktree：`probe-box-prim-null` @ `wt-boxnull` | 基于：origin/main `fa7560e21` (#833)
> 类型：`fix`（**vm** —— 一处运行期诊断；编译器与格式都不动）
> 授权：User「那第二个要怎么处理？」→ 方案获批 →「合并了，请你开始」

## 背景：一条理由已经作废、却还在放行的分支

`builtin_box_prim` 收到 `Value::Null` 时**原样返回 null**，是 `fix-box-null-nullable` (#717)
加的。它当时的论证是：

> `?` 纯标注、类型解析期彻底擦除 ⇒ `int x = null;` 能编能跑 ⇒「int 槽里装着 Null」是**语言
> 允许的状态** ⇒ 装箱点**分不清**「用户给 `int?` 赋了 null」（合法且常见）与「读到未初始化
> 槽位」（bug），用一条分不清好坏的信号去挡 bug，代价是每个正常用 `int?` 的人都撞上内部错误。

**这套前提已经全部作废**（`enforce-value-type-non-null` #741）：

| #717 依赖的前提 | 今天 |
|---|---|
| `int x = null;` 能编能跑 | ❌ **E0475** |
| `int? n = null;` 合法且常见 | ❌ **E0476**（值类型不许 `?`） |
| 「int 槽里装着 Null」是允许的状态 | ❌ 不变式：**值类型的存储槽永不含 `Value::Null`** |
| 装箱点分不清好坏信号 | ❌ 信号现在是**明确的**：只可能是 VM / 编译器缺陷 |

⇒ 那条分支不该继续无声。**这不是 bug 修复，是把一个已经无意义的沉默改成诊断。**

## 为什么到今天才做（时序约束）

`enforce-value-type-non-null` 归档复核时发现「泛型型参字段没有零值」，那是当时**唯一**能把
Null 送进装箱点的来源。在它修好之前加检查，等于把一个静默错值变成**在飞的崩溃**。

前置条件已满足（2026-09-25 实跑核实，interp + jit 一致）：

| 形态 | 修者 | 现状 |
|---|---|---|
| 直接实例化 `GBox<int>().V` | #822（分配点按实例化取零值） | ✅ 0 |
| 继承 `class DInt : GBox<int>` | **#831**（泛型实例化成为运行期真正的类型） | ✅ 0 |
| 泛型 struct `struct GS<T> { T F; }` | **#831** | ✅ 0 |

## 方案：debug 报错、release 放行

```rust
if matches!(inner, Value::Null) {
    if cfg!(debug_assertions) { bail!("__box_prim received Null — …不变式被破…"); }
    return Ok(Value::Null);
}
```

**为什么 debug 侧收益特别大**：e2e golden 语料默认跑的就是 **debug 版 z42vm**
（`scripts/test/xtask_test_vm.z42` 的 `_activeVm(root, "debug")`）⇒ 整个 golden 语料 + 单测
在 CI 里成为这条不变式的探测器，缺陷当场炸在**发生点**，而不是让 Null 顺流而下、
炸在毫不相干的地方（`PrimModel` 静态表读成 Null 那类）或干脆静默给出错值。

**为什么 release 仍放行**：摸底零命中只说明**现有语料没踩到**，不等于不存在
（反射 / interop / 平台特有路径覆盖不满）。不拿用户的崩溃去换我们的诊断能力。
若它在若干版本里一直不响，再提升为无条件报错。

**为什么不报用户级异常**（与拆箱那侧的差别）：#746 让 `(int)o` 抛
`NullReferenceException` / `InvalidCastException` 是对的 —— 那是**用户写的**转换。
这里的 Null 不是用户的错，报用户级异常会把责任指向错误的一方，用户照提示改自己的代码只会白忙。

## 进度概览

- [x] 0 前置：实跑核实两格已修（#831）
- [x] 1 摸底：探针设成**无条件 bail**（最响）跑全量
- [x] 2 最终形态：debug bail / release 放行
- [x] 3 正面对照单测（证明这门真的会响）
- [x] 4 补 `value_field_zero` 的两格（另一件跟进项）—— 顺带纠正一条判断，见下
- [x] 5 全量 GREEN + 指纹判定 + 文档 + PR

## 1 摸底（探针相）

把分支临时改成**无条件** `bail!("PROBE_BOXNULL: …")`（debug 与 release 两侧都响），
跑全量 `xtask test`：

```
命中 0 / 全量 ✅ GREEN（9m58s、15 stage 全过）
```

⇒ 今天没有任何一条路径把 Null 送进装箱点。

🔴 **摸底期栽过一次「拿错二进制的假红」，记下来**：第一轮跑 `xtask test` 时我给它传了
`Z42_HOME=<种子 SDK>`，而 **`Z42_HOME` 就是 `--toolchain`** ⇒ golden regen 用的是**种子里的旧
z42c**，于是 `foreach_dispose_optional` / `closed_generic_cast` / `generic_class_identity` /
`runtime_config_query` **四条红** —— 它们恰好是最新四个 PR（#823/#830/#831/#832）的用例。
去掉 `Z42_HOME` 后 365 ok / 0 failed。
⇒ **判据：冷树首轮的红，先问「我喂给它的是哪个编译器」**；
`Z42_HOME` 只在**供种**步骤该给，跑 `test` 时给它等于拿旧工具链验新代码。

## 3 正面对照（不可省）

全量零命中 ⇒ **必须**证明这门不是「恒不响的门」。`corelib/convert_tests.rs` 三条：

| 用例 | 钉什么 |
|---|---|
| `box_prim_null_is_an_error_in_debug` | debug 下喂 `Value::Null` **必须**报错，且消息里说明「不变式被破」 |
| `box_prim_null_passes_through_in_release` | release 下行为与 #717 之后**一字不变** |
| `box_prim_does_not_intercept_a_real_integer` | 🔒 真整数不被这道门拦住（错误文本证明它已走过 Null 分支） |

⚠️ 第三条不能写成「装箱成功」：裸 `VmContext` 没有类型注册表，`Std.Int32` 这个 wrapper
查不到，装箱最终仍失败 —— 但**失败在后一步**。真正的成功路径由整个 golden 语料端到端覆盖
（`object o = 42` 到处都是），不在这个脚手架里重造。

## 4 `value_field_zero` 补两格 —— 顺带发现「存储对了 ≠ 编译期类型代换了」

该用例此前只有非泛型 + 直接实例化的泛型格。补**继承格**与**泛型 struct 格**时撞出一条
**先前判断不准**的地方（我一度据运行期探针说「两格都修好了」）：

| 格 | 存储零值 | 编译期类型 |
|---|---|---|
| `struct GStruct<int> { T F; }` | ✅ 零值 | ✅ 已代换成 `int`（`F + 1` / `if` 都能写） |
| `class DInt : GBox<int> {}` 的继承字段 | ✅ 零值 | 🔴 **仍是 `T`** |

继承格今天编不过的两行（各报 **E0402**）：

```
Assert.Equal(1, d.V + 1);   // operator `+` requires numeric operand, got `T`
if (dbl.V) { … }            // `if` condition must be `bool`, got `T`
```

⇒ 那是**编译期代换**的缺口（另一条线：`complete-generic-instantiation`），与本页钉的
**存储零值**是两件事。用例里把这两行注掉并写明「缺口补上后取消注释」。
⚠️ 注释还写明这两格钉的是 **#831** 的成果、不是分配点那层（#822）的回归 ——
否则下一个人会拿它去判错东西。

⭐ 教训：**运行期探针只能证明「存储对了」**。我最初的核实只写了
`((object)d.V) == null` 和 `d.V == false` 这类**对 `T` 也合法**的表达式，
于是漏看了编译期那一半 —— 要探「类型代换了没有」，探针必须**用上只有具体类型才允许的操作**
（算术 / `if` 条件 / 赋给具体类型的局部）。

## 5 收尾
- [x] 5.1 `xtask test all` **GREEN**（全 15 stage 通过）+ `cargo test --lib`（**debug**）
      **1396 + 21 passed / 0 failed**
- [x] 5.2 指纹判定：**不 bump**。`CompilerFingerprint` 管的是「同一份源码 + 同样格式，编出的
      **字节**却变了」。本变更编译器一行未改、IR 与 zbc 完全相同，变的只是 VM 在 debug 档下
      对一个**本不该发生**的输入多报一条错 ⇒ 既无「哈希不变而发码变」的缓存条目，也无格式变化。
      （连诊断都不算变：release 行为一字未变，debug 那条针对的输入在现有语料里零命中。）
- [x] 5.3 文档：`internals/runtime/object-abi.md` 新增「反方向：装箱点收到 `Null` = 不变式被破」
      一节（含 debug/release 两档表 + 为什么不报用户级异常）；同页把「仍未覆盖的两格」改成
      **已由 #831 补上**，并写明「存储对了 ≠ 编译期类型代换了」；
      `archive/2026-09-18-fix-box-null-nullable/` 的补记同步更新
- [x] 5.4 归档（阶段 9，在本 PR 内）→ `archive/2026-09-25-alarm-on-boxing-null-value-slot/`
