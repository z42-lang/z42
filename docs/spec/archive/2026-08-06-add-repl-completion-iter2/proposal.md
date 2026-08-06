# Proposal: REPL 补全迭代二——Tab List 模式修复 + 基元变量成员补全

## Why
- **Bug：多按 Tab 倒退**（User 报告）。rustyline 默认 `completion_type: Circular`——反复 Tab 在候选间
  循环、转一圈**回到原始输入**（变短），观感即「Tab 越按越退」。代码 REPL 应用 **List 模式**（bash 式：
  首 Tab 补最长公共前缀，再 Tab 列候选，不破坏性循环）。
- **基元变量 `obj.` 成员补全缺失**（迭代二）。`s.`/`n.`（会话变量是 string/int）不补成员——
  `builtin_repl_member_names` 只对堆对象反射，基元返回空。而基元反射本身可用（实测 string 35 / int 10 成员）。

## What Changes
- **Tab List 模式**（`corelib/repl.rs`）：编辑器用 `Config::builder().completion_type(List)` 构造，替代默认 Circular。
- **基元成员补全**（`corelib/repl.rs`，`builtin_repl_member_names`）：基元会话变量映射到 `GetType()` 同款
  canonical stdlib 类（`Std.String` / `primitive_class_name` 的 `Std.Int32` 等 / 装箱的精确类）——**镜像
  `object::builtin_obj_get_type`** 使 `make_type_from_name` 能解析，反射出真实成员集。

两处均在 `repl.rs`（Rust）；不碰 `Completer.z42`（基元分支已经走 `_memberComplete` → `Repl.MemberNames`）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/repl.rs` | MODIFY | 编辑器 `with_config(CompletionType::List)`；`builtin_repl_member_names` 基元分支映射到 canonical 类名 |
| `docs/design/toolchain/repl.md` | MODIFY | 补全段补「List 模式」+「基元变量成员」 |

**只读引用**：
- `src/runtime/src/corelib/object.rs` — `builtin_obj_get_type` 的基元→类名逻辑（镜像它）
- `src/runtime/src/interp/exec_vcall.rs` / `interp` — `primitive_class_name`；`metadata::well_known_names::STD_STRING`

## Out of Scope
- 命名空间名补全（`.using Std.` → `Std.IO`）——需含点前缀 + 替换区间处理，留后续迭代。
- `.members` / `.type` / `.mode` 元指令、错误行号高亮——留迭代三。
- 数组变量 `arr.` 成员补全——本轮基元只覆盖 string/int/double/bool/char/boxed。

## Open Questions
- 无。
