# Tasks: native 字符串化路径派发 `ToString`

## 实施

- [x] `interp/dispatch.rs`：`obj_to_string` 新增 `Value::BoxedStruct` 臂（复用 `resolve_vcall`，
      **不用 `resolve_by_candidates`**，见 design D2）+ 抽出 `exec_to_string` + 新增 ctx-only
      包装 `stringify_dispatch`（`module` 取自 `ctx.core.module`，取不到回落 `value_to_str`）
- [x] `corelib/io.rs`：`builtin_println` / `print` / `eprintln` / `eprint` 四个改用 `stringify_arg`
      （此前签着 `_ctx` 却用无 ctx 的 `value_to_str`）
- [x] `interp/exec_value.rs::add`：混合臂对 `Object` / `BoxedStruct` 派发（`module` 从
      `exec_instr.rs` 穿进来）；`Str+Str` 融合分配与整数快路一字不动
- [x] `jit/helpers/arith.rs::jit_add`：对称的 `jit_stringify`
- [x] `jit_str_concat` **不动**：它是「两侧静态都是 string」的快路，非字符串操作数本就走 `Add`

## 验证

- [x] 四条路 × 四种类型矩阵全部收敛（class / record class / 单字段 struct / 双字段 struct）：
      `WriteLine` / 拼接 / 插值 / 显式 `.ToString()` 给**同一个答案**
- [x] 无覆写类型 → 短类型名（`BareC` / `BareS`），与既有 golden 钉的 ①② 一致
- [x] 非对象操作数逐条不变：`42` / `3.5` / `true` / 裸串 / `[1, 2]`（数组仍递归）/ `null`
      / enum 打成员名（`Blue`）
- [x] **interp 与 jit 输出逐字节 `diff` 为空**
- [x] 既有 golden `structs/struct_tostring_paths.z42` 仍绿（interp + jit）
- [x] 新增 Dir 模式 golden `src/tests/structs/tostring_native_paths/`
      （`WriteLine` 只能比对 stdout ⇒ 必须 Dir 模式；已确认 runner 认领：日志 `OK: tostring_native_paths`）
- [x] `xtask test` 全仓 **✅ GREEN**（7m55s，零 `✗`）
- [ ] `cargo test --lib`（debug、不带过滤）
- [x] **性能**：拼接最重的宏观负载 `compiler` 档（z42c 自举）改前/改后**都是 1m20s**；
      3M 次 `"x" + i` interp 0.78s / jit 0.42s（混合臂只多一次 Value 判别）

## 受影响的既有期望（**已更新**）

`src/libraries/z42.scripting/tests/repl_{trailing_semicolon,generic_decl,target_typed_new}/expected_output.txt`
—— REPL 回显走 `_fmt(object v) { return "" + v; }`（注释自称「MVP：ToString via concat」），
改后 `Std.Collections.List{...}` → `List`、`Repl.R1.Pt{...}` → `Pt`。逐行 diff 确认**只有这几行变**。

⚠️ **这 11 个 fixture 是仓里显式声明的孤儿**（`z42.scripting.z42.toml` 的 `[tests] auto = false`
+ 注释「两套发现规则都不认领它们，实际没有任何东西在跑」）⇒ 全仓 GREEN **不是**它们通过的证据，
我是手工 `Z42_LIBS=artifacts/.z42/libs z42 run …/driver.z42` 取的真值。**「零命中」在此有解释。**

## 文档

- [x] `docs/reference/src/language/structs.md`：「`Console.WriteLine(s)` 不走 `ToString`」整节改写
      （并订正原表把 `"x" + s` 一律记作 ✅ —— 那只对**双字段** struct 成立）
- [x] `docs/learn/src/types/structs-records.md`：删掉「`WriteLine` 例外」段与小结那条
- [x] `docs/roadmap.md`：落地行 + Deferred

## 登记的 Deferred

- `propagate-tostring-exception` —— `ToString` 抛异常时现在产 `<exception: …>` 字符串
  （沿用 `obj_to_string` 既有约定）。改成传播会让 `Console.WriteLine` / 拼接变成可抛点，
  是独立取舍。
- `repl-value-display-contract` —— REPL 回显 `List` 仍不如 `[1, 2, 3]` 有用；那是 REPL 自己的
  展示契约，且要先把那 11 个孤儿 fixture 接进门禁。
