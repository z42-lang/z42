# Tasks: lint 遍历补齐局部函数体

> 状态：🟢 已完成（2026-09-09）｜ 创建：2026-09-09 ｜ scope: compiler（无格式 bump）

| # | 阶段 | 状态 |
|---|---|---|
| 1 | `_walkStmt` 加 `LocalFunctionStmt` 递归 + 修尾注 | 🟢 已完成 |
| 2 | analyzer_tests 加回归门 | 🟢 已完成 |
| 3 | 验证（新门 PASS + 退回对照 FAIL + `test compiler` + 完整 GREEN） | 🟢 已完成 |
| 4 | 归档 + 随 PR 提交 | 🟢 已完成 |

## 详情

- [x] 1 `AnalyzerDriver._walkStmt`：`LocalFunctionStmt` 分支递归 `lf.Decl.Body`（仅下钻体、不额外
      分派 MethodDecl）；尾注「无子语句」清单去掉 `LocalFunction`、补 `Deconstruct`。
- [x] 2 `test_empty_catch_inside_local_function_reported`：空 catch 藏在局部函数体内 → 观察
      CatchClause 的 analyzer 必须报 Z9002。
- [x] 3 验证：
  - 新门修后 PASS；analyzer 单元 36→37 passed/0 failed。
  - 退回对照：只 stash `AnalyzerDriver` 的修、保留测试重建 → 新门 `FAIL: values not equal` + exit 1
    ⇒ 门真钉在修上。
  - `xtask test compiler` REAL_EXIT=0；完整 GREEN `✅ GREEN — all stages passed`。
- [x] 4 归档 `changes/`→`archive/2026-09-09-fix-analyzer-walk-local-function/`，随 PR 同批。
