# Tasks: fix-repl-trailing-semicolon

> 状态：🟢 已完成 | 创建：2026-08-15 | 完成：2026-08-15 | 类型：fix（toolchain / scripting）

**变更说明：** REPL 里带尾随 `;` 的表达式/语句（`l.Add(5);` / `n = 5;` / `1+2;`）报 E0202/E0201
且该轮副作用丢失；修为剥离单个尾随 `;`，使其与不带 `;` 等价。
**原因：** `Script.Eval` 对非声明输入按「表达式包裹 `return (X);`」+「语句回退 `X; return null;`」两路
编译；尾随 `;` 让前者成 `return (X;);`（内层 `;` 破坏括号）、后者成 `X;; return null;`（`;;` 空语句
被 parser 拒）→ 两路皆炸。剥掉尾随 `;` 让两路都正确。
**文档影响：** `docs/design/toolchain/repl.md` 求值编排段补「尾随分号归一」说明。

- [x] 1.1 `src/toolchain/scripting/src/Script.z42` `Eval`：build 源前剥离 `rewritten`/`trimmed` 单个尾随 `;`
- [x] 1.2 回归测试 `src/toolchain/scripting/tests/repl_trailing_semicolon/`（driver.z42 + expected_output.txt）
      —— 已验证：pre-fix 输出 E0202/E0201（DIFFERS），post-fix 全对
- [x] 1.3 文档同步：`docs/design/toolchain/repl.md` 求值编排段
- [x] 1.4 手工 REPL e2e（fresh z42i apphost + fresh libs）：方法调用/赋值/多语句/裸表达式/无分号对照 —— 全通过
- [x] 1.5 完整 `xtask test` GREEN —— ✅ all stages passed (C#-free)，self-host 5/5 gen1==gen2

## 备注
- 二级 bug「`C c` 未定义类型不报错」经诊断是 **z42c.semantics 类型检查器缺陷**（`ResolveTypeP`
  对未解析类型名一律返 `Z42UnknownType` 且 `TypeChecker` 对 Unknown early-return 跳过诊断），
  非 REPL bug、不同子系统 → 拆为独立 change（PR B，lang/semantics，spec 先行）。本 change 不含。
- GREEN gate 不编 z42.scripting/z42.interactive（[green-gate-skips-scripting-interactive]）→ 本 fix
  的正确性以 fresh toolchain 构建 + 手工 REPL e2e + driver 回归测试为准；`xtask test` 验证未回归其它。
