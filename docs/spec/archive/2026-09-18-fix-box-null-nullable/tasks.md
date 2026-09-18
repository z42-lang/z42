# Tasks: 装箱 null 的可空值类型抛内部错误

> 状态：🟢 已完成 | 完成：2026-09-18

- [x] 1 `builtin_box_prim` 加 `Value::Null` → `Value::Null` 分支（放在幂等分支旁）
- [x] 2 golden `src/tests/types/box_null_nullable.z42`
- [x] 3 `reference/language/types.md`「可空标记」补一节：装箱得 null，但算术仍会抛

## 验证

| 项 | 结果 |
|---|---|
| golden × interp / jit | 两种模式 exit 0 |
| **golden 对修复前的 VM** | ✅ 两种模式都抛 `__box_prim: expected integer value, got Null` |
| `cargo test --lib`（debug） | 1359 passed / 0 failed |
| 全量 `xtask test` | 见提交时的记录 |
| 文档里 4 条输出 | 逐条实跑，字面一致 |

⚠️ 本机跑 `xtask test` 必须带 `RUSTUP_TOOLCHAIN=1.98.1`。不带的话默认 rustc 1.88.0
过不了 cranelift 的 MSRV，**xtask 会在第一步就 bail 但退出码仍是 0**——看起来像跑完了。
第一次就踩了这个，靠数输出行数（35 行）才发现。

## 判断依据：为什么这是 bug

`?` 是纯标注、类型解析期擦除，`int x = null;` 本身就能编能跑 —— 「int 槽里装着 Null」
是**语言明确允许的状态**，不是 VM 不变量被破坏。既然允许它存在，就必须定义它被装箱时的
行为，而不是漏到一条内部实现名（`__box_prim`）里。C# 在同一情形下给 `o == null`。

同一件事此前在**插值**下正常（打 `null`）、在**装箱**下抛内部错误，口径本身就不一致。

## 取舍：少了一处噪声告警

`interp/exec_array.rs:75` 的注释记着：泛型数组未写槽位读出 Null 那桩 bug，当年正是靠这条
`__box_prim: got Null` 暴露的（根因已由 `fix-generic-array-value-zero-init` 在源头修掉）。

判断它**不是一道有效防线**：因为 `?` 完全擦除，装箱点无法区分「用户给 int? 赋了 null」
（合法且常见）与「读到未初始化槽位」（bug）—— 两者在 `Value` 层面完全相同。
用一个分不清好坏的信号挡 bug，代价是每个正常用 `int?` 的人都撞上一条内部实现名。
未初始化那类仍会在后续使用点暴露（`x + 1` 报 `type mismatch in arithmetic: Null vs I64(1)`，
信息量不比原来少）。

## 合并后必须跟着改

`docs/learn/src/basics/operators.md`（第 6 章，PR #716）有一段：

> 另外，`int?` 这类"可空的值类型"目前只在**字符串插值**里显示得正确（`$"{n}"` 打出
> `null`）；直接 `Console.WriteLine(n)` 会在运行期出错。需要打印时走插值。

本 change 合并后这句**不再成立**（`Console.WriteLine(n)` 会打印 `null`）。两个 PR 谁先合
都要记得改掉——留着就是教一个已经不存在的坑，与第 6 章那段 `==` 警告是同一类问题。

## 顺带发现，未修

- `n?.ToString()`（n 为 `int?`）报 `E0402: unsupported call form`。
- 真正的可空值类型（`Nullable<T>` 语义 + 静态空安全检查）是语言特性，不在本 fix 范围。
  本 change 只让「已经允许存在的状态」有定义好的装箱行为。

## 本机 gate 的既有噪声（与本 change 无关，已用基线对照证明）

本机 `xtask test` 的 golden 段报 `0 passed, 325 failed`。**325 条里零条是语义差异**——
全部是同一形状：

```
expected: <empty>
actual:   WARN z42::native::ext: ext: ignoring unknown lib `repl`
```

**基线对照**：把 `convert.rs` 换回 main 原版、重编、跑同一个 gate → **完全相同的
`0 passed, 325 failed`，同样零条非-repl 失败**。所以与本 change 无关。

成因（已查到，值得单开一个 change）：

- `native/ext.rs:167-175` 的 `native_search_paths()` 第 3 条是 `exe.parent()`
  ——裸 cargo target 目录（2026-05-24 为 `libz42_compression.dylib` 加的）。
- `libz42_repl.dylib` 是同 workspace 的 cdylib，**正好也在那个目录**，于是被 eager ext
  扫描器捞到 → `load_one` 落到 `other =>` 分支 → `tracing::warn!` → 污染 stderr。
- 讽刺的是 `corelib/repl_native.rs:18` 的注释明确声称「把它挡在共享 `bin/` 外，
  就不会让 ext 扫描器每次警告 `ignoring unknown lib repl`」——这话在 **SDK 布局**下成立，
  在**开发树布局**下不成立：那里两个 dylib 就在同一个目录。
- `RUST_LOG=error` 压不掉（实测）；`xtask build sdk` 之后仍然如此（实测）。

修法方向：`load_one` 应当**直接跳过** `repl`（它有自己的 colocated 探测路径
`repl_native::candidates()`），现在是白 dlopen 一次、警告一次、再把库 park 住。

另记一条假故障：`xtask test` 不带 `RUSTUP_TOOLCHAIN=1.98.1` 会在第一步因 cranelift MSRV
失败，**但退出码仍是 0**，看起来像跑完了。靠数输出行数（35 行）才发现。
