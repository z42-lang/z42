# tasks: builtin-arity-local-validation

> 类型：**fix / hardening**（builtin 的 arity 错配不再静默吸收成 `Value::Null`）｜ 创建：2026-09-28
> 出身：审计 **R3** 的 **⑥ 缺参**。前序：#912（④ void）· #906/#908/#917（⑤ 与写侧类型校验）。

## Why

22 处 `args.get(N).cloned().unwrap_or(Value::Null)` ⇒ **「少传了参数」与「显式传了 null」
完全无法区分**。arity 错配只可能来自 stdlib 的 `[Native]` 声明与 Rust 实现不一致
（编译器会校验 extern 调用点），而那种维护错误此前被**静默吸收成 Null**，
表现为 builtin 深处一个莫名的 Null —— 离现场很远。

⭐ **`__contains` 那一格最能说明为什么必须靠 arity 而不是「判断值是不是 Null」**：
它的数组分支缺参时会去**搜索 Null**，而 `list.Contains(null)` **本身是合法操作** ——
这两件事只有 arity 校验分得开。

## 🔴 先更正我自己推迟这件事时给的理由

上一轮我把「加一道 arity 对账门」判为「需要独立设计、假红风险高」，**理由给错了**：

| 我当时说的 | 实测（2026-09-28） |
|---|---|
| 「**确实有变长 builtin**（`&args[1..]` / `args.iter()` / 6 处按 `args.len()` 分支）⇒ arity 不是单个数字」 | **错**。逐条查完 **没有真正变长的 builtin**：`&args[1..]`（`type_query.rs:293`）只是给 helper 传「`.first()` 是 `c`」的**切片技巧**、固定 2 参；`args.iter().find_map(..)`（`attributes.rs` ×5）是**松散地找一个 Str 参数**；`args.len()` 那 6 处里 4 处是 `invoke_arity_check`，校验的是**被反射调用的 z42 函数**的 arity，与 builtin 自身无关；`array.rs:279` 则是 `__array_clone` **自己在校验**。 |
| 「从 Rust 源扫 `args.get(N)` 属启发式 ⇒ 假红风险高」 | **结论对，但真正的理由是另一个**：307 个 builtin 里**只有 80 个**的函数体真的字面索引 `args`（其余用 helper / 解构 / 切片）⇒ **压根没东西可提取**；而那 80 个里还有 8 处「不一致」**全是我提取器自己的错**（如 `__process_run used=15 expect=0`，body 切分越界）。 |

⇒ **门这条路确实不通**（会假红，而本仓的教训是假红比没门更坏），但不是因为变长。

## What Changes：局部校验，不是门

仓里本来就有先例 —— `__array_clone` 一直这么做：

```rust
if args.len() != 1 { bail!("__array_clone: expected 1 argument (this), got {}", args.len()); }
```

加一个统一 helper `corelib::expect_args(who, args, want)`，给 **11 个 builtin** 补校验，
**校验之后直接下标取参**（`args[N]`），于是 `unwrap_or(Value::Null)` 这个形态随之消失。

| builtin | arity | 文件 |
|---|---|---|
| `Array.CreateInstance` / `Array.SetValue` | 2 / 3 | `corelib/array.rs` |
| `Bench.BlackBox` | 1 | `corelib/bench.rs` |
| `Contains`（legacy，数组分支） | 2 | `corelib/io.rs` |
| `PropertyInfo.GetValue` / `SetValue` | 2 / 3 | `reflection/accessors.rs` |
| `FieldInfo.GetValue` / `SetValue` | 2 / 3 | `reflection/accessors.rs` |
| `MethodInfo.Invoke` / `GetGenericArguments` / `MakeGenericMethod` | 3 / 1 / 2 | `reflection/invoke.rs` |
| `ConstructorInfo.Invoke` | 2 | `reflection/invoke.rs` |

⚠️ `MakeGenericMethod` / `ConstructorInfo.Invoke` 此前看着「已有 `args.len()` 校验」，
实测那是 `invoke_arity_check` —— 校验**被反射调用的 z42 函数**的形参数 vs 用户实参数组，
**与本 builtin 收几个参数是两件事**。注释里写明了，免得下一个人再误判。

⇒ 全仓 `args.get(N)/first().cloned().unwrap_or(Value::Null)` **清零**。

**不改用户可见行为**：arity 错配从 z42 代码不可达（编译器校验 extern 调用点）⇒ 不需规范先行。

## 顺带抓到一条断言旧哨兵的测试

`bench_tests::black_box_no_arg_returns_null` 断言「**零参**调用返回 `Value::Null`」——
那正是 ⑥ 这个混用本身（与 #912 抓到的 `drop_unknown_slot_is_silent_null` 同款形状）。
已改名 `black_box_arity_mismatch_raises`、断言报错，并配一条回归门（正常一参原样返回）。

## Tasks

- [x] `expect_args` helper（注释里写明「为什么不做成门」）
- [x] 11 个 builtin 补校验 + 改直接下标；`unwrap_or(Value::Null)` 形态清零
- [x] 改正断言旧哨兵的测试 + 回归门
- [x] **阴性对照**：把 `expect_args` 的判据改成恒假 ⇒ 只有 `black_box_arity_mismatch_raises`
      变红，其余 4 条回归门保持绿
- [x] `cargo test --lib`（不带过滤）1375/0 · 4 个 feature 组合全过
- [x] `xtask test e2e` 748/87/3 · `test stdlib` 347 组 · `test compiler` 24/24 + 不动点 3/3
- [ ] GREEN：CI 全矩阵绿

## 不做

- **不做「表 ↔ stdlib」的 arity 对账门**（理由见上：会假红）。若将来要做，正路是让表带
  `(min_arity)` 并由门从 stdlib 声明**反向**核对 —— 那是 307 项的机械改动，需先有它能抓到
  真实缺陷的证据。
- **不动 `io.rs` 的 `__contains` 的字符串分支**（它早就经 `arg_str(args, 1, ..)` 校验了）。
