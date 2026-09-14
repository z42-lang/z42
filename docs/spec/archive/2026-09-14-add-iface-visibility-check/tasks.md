# tasks — add-iface-visibility-check

- 🟢 阶段 0 爆炸半径：`rm .cache` 全量 `build stdlib`+`build compiler`+`build test` → E0412 命中 0
- 🟢 IMPL：`InheritanceResolver._checkOneIfaceMethod` 补 `cm.Visibility != "public"` → E0412
- 🟢 测试：`constraint_tests.z42` 5 条门（4 负例 private/internal/protected/无修饰 + 1 public 守卫）
- 🟢 退回对照坐实（`if (false && …)` 禁用 → 4 负例恰 FAIL、public 守卫 + 既有 static 门 PASS）
- 🟢 文档：`docs/book/src/language/generics.md` 接口满足性节补可见性
- 🟢 User 裁决口径：严格（对齐 C#，无修饰默认 private 也拦）— 2026-09-14
- 🟢 GREEN：完整 `xtask test`（interp）全 stage 绿 + `test e2e --dir cross-zpkg --mode jit` 45/0 + `test stdlib --mode jit` + `test bootstrap` NO violation
- 🟢 自举不动点 3/3 gen1==gen2（sections byte-identical）
- 🟢 归档 + PR
