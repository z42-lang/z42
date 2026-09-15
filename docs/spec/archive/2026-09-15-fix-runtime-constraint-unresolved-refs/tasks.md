# tasks — fix-runtime-constraint-unresolved-refs（C1-only）

状态：🟢 完成

## C1 — check_one 接口感知 soft-allow ✅

- [x] `check_constraint_refs`：给 `check_one` 传 `soft_allow_unresolved`（interface / func-sig → true，
      base_class → false）
- [x] `check_one`：未解析且 soft → Ok；否则 bail；删 `I`+大写启发式
- [x] 单测：`verify_allows_non_ifoo_interface_reference`（Comparable 通过）+
      `verify_rejects_unresolved_ifoo_base_class`（IFoo 命名基类仍 bail）+ 更新 `verify_allows_interface_like_name`
- [x] 退回对照：stash constraints.rs → 两新测试精确变红（6 passed / 2 failed），复原后全 8 绿

## C2 — 已查实无 bug，丢弃（见 proposal「已查实非目标」）✅

## GREEN ✅

- [x] `cargo test`（全量串行 `--test-threads=1`）0 失败 —— 并行时 3 个 `gc::arc_heap` mode 测试
      因全局 GC mode static 跨模块串味假红（隔离/串行全过），pre-existing、与本 diff 无关
- [x] `Z42_PORTABLE_VM=<my vm> RUSTUP_TOOLCHAIN=1.98.1 ./xtask test`（interp）全 13 stage GREEN
- [x] `test e2e --dir cross-zpkg --mode jit` PASS + `test stdlib --mode jit` 336/0
- [x] `test bootstrap` NO staged-bootstrap boundary violation + repo z42c self-build OK

## 文档 ✅

- [x] `docs/book/src/language/generics.md`：更新 verify_constraints 描述（删 `I<Upper>` 启发式、
      改按引用种类 soft-allow）

## 归档

- [x] 移 `changes/` → `archive/2026-09-15-fix-runtime-constraint-unresolved-refs/`，tasks 改 🟢
- [ ] PR（body 含 What/Why + 验证 + 页脚）
