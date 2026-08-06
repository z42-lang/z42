# Tasks: add-repl-completion-iter2

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：fix + feat（REPL 补全；均在 corelib/repl.rs）

## 进度概览
- [x] 1. Tab List 模式（修多按倒退 bug）
- [x] 2. 基元变量成员补全
- [x] 3. 验证 + 文档同步

## 1. Tab List 模式（repl.rs）
- [x] 1.1 编辑器 `Editor::with_config(Config::builder().completion_type(CompletionType::List).build())` 替代 `::new()`（默认 Circular）

## 2. 基元变量成员补全（repl.rs `builtin_repl_member_names`）
- [x] 2.1 基元 Value 分支映射到 canonical 类名（`Str→Std.String`、`I64/F64/Bool/Char→primitive_class_name`、`Boxed→b.class`）——镜像 `object::builtin_obj_get_type`
- [x] 2.2 实测基元反射成员：string 35 / int 10（`GetType().GetMembers().Length`）——路径命名一致，成员补得出

## 3. 验证
- [x] 3.1 `cargo build --release`——两处编译通过
- [x] 3.2 完整 GREEN（无 Z42_HOME）全绿：e2e 224/0 + cross-zpkg 8/0 + multi-exe 1/0 + stdlib 全绿；compiler fixpoint 冷种子 drift 3/5 → build compiler 收敛 → 5/5
- [x] 3.3 文档同步：`docs/design/toolchain/repl.md`（补全段补 List 模式 + 基元变量成员）
- [ ] 3.4 交互手感验收（User，需真实终端）：反复 Tab 不再倒退（List 列候选）；`s.`/`n.`+Tab 补出 string/int 成员

## 备注
- Tab List 与基元成员均为交互特性：本机自动验证覆盖 cargo 编译 + 基元反射实测 + GREEN 无回归；手感由 User 终端验收。
- 独立 worktree `z42-repl-iter2`（基于 origin/main f0c890c7，含 #132）。
- 命名空间补全 / 元指令 / 错误行号留后续迭代（见 proposal Out of Scope）。
