# Proposal: `Console.WriteLine(obj)` 与字符串拼接走 `ToString` —— 四条路说法收敛为一条

> 状态：✅ 已实施（2026-09-25）。类型：**vm**（native 字符串化路径改为派发用户 `ToString`）⇒ 走完整流程。

## Why

**同一个对象，四条字符串化路径今天给四个答案。** 2026-09-25 实测矩阵（main + 新建 SDK）：

| 路 | 单字段 struct | 双字段 struct（blob） | class | record class |
|---|---|---|---|---|
| ① 显式 `x.ToString()` | ✅ `S1!2` | ✅ | ✅ | ✅ |
| ② 插值 `$"{x}"` | ✅ | ✅ | ✅ | ✅ |
| ③ **拼接 `"" + x`** | 🔴 `S1{...}` | ✅ | 🔴 `C{...}` | 🔴 `RC{...}` |
| ④ **`Console.WriteLine(x)`** | 🔴 | 🔴 `S2{...}` | 🔴 | 🔴 |

`fix-struct-tostring-paths`（既有 golden `structs/struct_tostring_paths.z42`）把 ①②③ 收敛过一次，
但**只补了 `_isBlobStruct`（字段数 ≥ 2）那一支**：

- ③ 对 **class / record / 单字段 struct** 仍落 `interp/exec_value.rs::add` 的裸 `value_to_str`
  ⇒ `corelib/convert.rs:258` 的 `format!("{}{{...}}", …)`。
  （单字段 struct 走引用路是坑点 ⑤ 的同一个 `FieldCount >= 2` 闸门。）
- ④ 对**所有**类型仍落 `corelib/io.rs:157-175` 的裸 `value_to_str`
  —— 那 4 个 builtin 签着 `_ctx` 却从不用它。

那份 golden 的抬头自己把 ④ 记成「另开」，理由是「要让 native builtin 重入 VM 派发，
**涉及 GC 根与可重入性**」。⚠️ **这条理由的前提已经不成立**：`ToStr` 指令（= 路 ②）
走的 `interp/dispatch.rs::obj_to_string` **本来就在 `exec_function` 重入 VM**，
且 `corelib` 里 repl / threading / `reflection/invoke.rs` 早已各自这么重入（含
`ExecOutcome::Thrown` 的处理范式）。⇒ 重入不是新风险，**只是这两条路没接上那个现成 helper**。

不做的代价：读者写 `Console.WriteLine(p)`（最自然的展示方式）拿到 `P{...}`，
而 `$"{p}"` 是对的 —— 规律无法自解释；REPL 更直接受害（`_fmt(object v) { return "" + v; }`
注释写着「MVP：ToString via concat」，而拼接对引用类型恰恰不派发）。

## What Changes

**把两条裸 `value_to_str` 接到现成的 `obj_to_string` 上**，并给它补齐装箱 struct：

1. `interp/dispatch.rs::obj_to_string`：新增 `Value::BoxedStruct` 臂。
   语义**照抄 `vcall_resolve.rs:223-236` 已验证的判据**：非 record 先探**自身槽位**
   `<TypeName>.ToString`，有真实现就 `exec_function`，没有才回落**短类型名**。
   🔴 不能用 `resolve_by_candidates` —— 它会回落 `Std.Object.ToString`，
   而那个 builtin 收到装箱 struct 直接抛 `__obj_to_str: expected an object`（前人踩过）。
2. `corelib/io.rs`：`builtin_println` / `print` / `eprintln` / `eprint` 改用 ctx 感知的
   字符串化（`module` 从 `ctx.core.module` 取，取不到则回落裸 `value_to_str`）。
3. `interp/exec_value.rs::add`：非字符串操作数改走 `obj_to_string`（需把 `module` 穿进来）。
   两条字符串快路（`Str+Str` 融合分配、整数 `int_binop`）**一字不动**。
4. JIT 对称件（`jit/helpers/value.rs` 的拼接助手）—— 只补一侧的话 interp 好而热代码不一致。

## 行为变化（**刻意的**，需 User 确认）

- 有 `ToString` 覆写的类型：`WriteLine` / 拼接从 `T{...}` 改为用户格式。
- **没有**覆写的类型：从 `T{...}` 改为**短类型名**（`Bare`）——
  与既有 golden 已钉的 ①② 行为一致（`Assert.Equal("Bare", $"{b}")`），四条路由此真正统一。
- 受影响的既有期望：`z42.scripting` 三个 REPL fixture（`Std.Collections.List{...}` 等 3 处文件）
  —— REPL 回显走 `_fmt` = 拼接，改后打 `List`。这正是它注释里想要的语义。
- ⚠️ **自指 `ToString` 会变成无限递归**（`override ToString() => "C" + this`）：今天因为不派发
  所以「碰巧」终止。C# 同样栈溢出 ⇒ 接受，但要在 reference 里写明。

## 不做什么

- **数组元素不逐个派发**：`value_to_str(Array)` 今天递归打元素，保持原样（C# 对数组只打类型名，
  z42 打 `[...]` 更有用，不在本刀动摇）。
- **`obj_to_string` 吞异常不改**：ToString 抛异常时现在产
  `<exception: …>` 字符串。要不要改成传播是独立取舍（会让 `WriteLine` 变成可抛点）⇒ 登记 Deferred。
- **REPL 自己的展示契约不另做**（如 `[1, 2, 3]`）——独立诉求。
