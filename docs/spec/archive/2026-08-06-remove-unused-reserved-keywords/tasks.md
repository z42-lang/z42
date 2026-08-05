# Tasks: 删除未接线的 Rust 风格保留关键字

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06

**变更说明：** 删除 Lexer 里注册但 parser/语义**零消费**的保留关键字 `fn/let/mut/trait/use/module/spawn/none`（仅保留唯一接线的 `impl`），一并清理 TokenKind 常量、vscode 语法高亮分类器与生成产物、lexer 哨兵测试。

**原因：** 这些是 z42 早期"Rust 风格保留字"占位，一直未接线成语法功能，却白白占用 `fn`/`let`/`mut` 等标识符名。`mut` 另有明确决策"永不引入"（与 GC 模型不兼容）。属死代码，清理让保留字表只留真实在用的。

**文档影响：** 无独立机制文档需改（这些关键字从无语法行为、无 book 页描述）。vscode grammar 是生成产物，随分类器同步。

## 删除清单（保留 impl）
| 关键字 | TokenKind | 消费方 |
|--------|-----------|--------|
| fn / let / mut / trait / use / module / spawn / none | Fn/Let/Mut/Trait/Use/Module/Spawn/NoneKw | **零**（仅 lexer 注册 + vscode 分类器） |
| impl | Impl | Parser `_parseImplDecl`（保留，不动） |

- [x] 1.1 `TokenKind.z42`：删除 Fn/Let/Mut/Trait/Use/Module/Spawn/NoneKw 常量（保留 Impl=63、ErrorKw=68）
- [x] 1.2 `Lexer.z42` `_initKeywords()`：删除对应 8 个 `_kw(...)` 注册（保留 impl）
- [x] 1.3 `scripts/install/xtask_install_vscode.z42`：分类器列表移除 spawn(control)/fn·let·trait·use·module(declaration)/mut(modifier)/none(literal)
- [x] 1.4 `src/toolchain/devtools/vscode/syntaxes/z42.tmLanguage.json`：重生正则去掉这些关键字（保持注册序）
- [x] 1.5 `lexer_tests.z42` `test_keyword_table_accessors`：哨兵 `fn`→`impl`（fn 已删、impl 仍在）
- [x] 2.1 GREEN：`xtask test` 全 stage 全绿（重建 worktree xtask 后跑；compiler 自举 ✔ / vscode-syntax ✔ / typecheck 95 passed / 全 stage ✔）
- [x] 2.2 连带修复：`typecheck_tests.z42` 两个用 `module` 作参数名触发关键字诊断的回归测试 → 改用仍存在的保留字 `impl`（覆盖不变）
- [x] 2.3 归档 + PR

## 备注
- 分支基于 **origin/main（9fcc9a8b, zpkg 0.34）**——本地 `main` ref (60845cf1, 0.33) 过期，逃逸分析 #115 已在真实 main。
- 自举安全：z42c 自身源码不用 fn/let/mut 等作标识符 → 种子（旧 z42c 带关键字）能编本 worktree 源、gen1==gen2 字节不动点不破。
- vscode gate 双向校验（`_vscodeValidateCategories` + `_vscodeNoGhosts`）：Lexer↔分类器已对称删除，一致性保持。
