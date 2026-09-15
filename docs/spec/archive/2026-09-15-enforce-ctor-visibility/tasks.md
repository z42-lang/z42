# Tasks: enforce-ctor-visibility

> 状态：🟢 已完成（User 2026-09-15 确认）｜ 创建：2026-09-15 ｜ 基于 main `47e43815d`

- [x] 1.1 单测 8 条（`access_control_tests.z42`）+ cross-zpkg 负例 `ctor_visibility_cross_pkg`（修前 nightly 编译主包成功 ⇒ runner 判红）
- [x] 2.1 `ConstructTyper` / `DeclBinder` 构造器可见性检查（D1）
- [x] 2.2 parser 主构造器修饰符 `"public"`（D2）；`z42c.syntax` AST dump 单测 5 处期望随之加 `public`
- [x] 2.3 `AccessChecker` 头注释订正（默认 private）+ 文案 `constructor`
- [x] 3.1 补 `public`：普查脚本批量 54 个文件 88 个构造器（tests / examples / z42.collections tests）+ `scripts/xtask_bench.z42` 的 `MicroBenchAgg` + 编译器单测源码串 6 处
- [x] 4.1 文档：book `compiler/access-control.md`（强制点表加两行 + 主构造器公有规则）、`language/constructors.md`（可见性一节）、cross-zpkg README
- [x] 5.1 全量 GREEN（`GREEN_EXIT=0`，基于 main `47e43815d`）；`xtask test bootstrap` ✅；夹具修正后 `test e2e --dir cross-zpkg` 51/51

## 补记（CI 发现）

- `src/tests/perf/scenarios/10_mono_vcall.z42` 的 `Box(int)` 漏补 `public`：perf 场景只在 CI `bench-regression` 里编译，
  本地 `xtask test` 与普查都没覆盖（普查挂在编译器里，只统计实际被编译的源码）。补上后把本地 GREEN 未编译的目录
  （`src/runtime` 夹具、`examples/*`、`scripts/package|install|hooks`、`src/toolchain`、`docs/spec`、`src/tests/perf`）逐文件扫描，无其它遗漏。

