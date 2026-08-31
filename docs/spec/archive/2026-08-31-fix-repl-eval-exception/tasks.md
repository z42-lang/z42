# Tasks: fix-repl-eval-exception

> 状态：🟢 已完成 | 创建：2026-08-31 | 完成：2026-08-31 | 类型：fix（bug 修复，不改语言语义）

**变更说明：** REPL 求值期抛出的运行异常（用户 `throw` / 除零 / 越界 / `int x = "s"` 之类
`__box_prim` 类型不符等）此前逃逸 `Script.Eval` → `Main()` 未捕获 → **终止 z42i 进程（exit 1）**；
现捕获为失败 `EvalResult`，REPL 继续接受下一条输入。

**原因：** `Script._evalExprOrStmt` 里 `Engine.Invoke` 无 try/catch，任何运行异常直接冒泡出 REPL 主循环。
用户报告：`z42 repl` 遇「编译报错」退出，应与单独 z42i 一样可继续。经实证定位：**编译错误（E0xxx）本已
被捕获、REPL 继续**；真正的杀手是**运行异常**（含看似类型错误的 `int x = "str"`，实为运行期 `__box_prim`）。

**文档影响：** z42.scripting/README.md（功能索引 Script.z42 行 + 关联文档登记 change）；Script.z42 文件头注释。
不新增 book 页（try/catch 简单错误恢复，非复杂机制流程）。

## 任务
- [x] 1.1 `Script.z42` `_evalExprOrStmt`：`Engine.Invoke` 包 try/catch(Exception)，捕获后返回失败
      `EvalResult(e.GetType().Name + ": " + e.Message)`；会话变量不推进。
- [x] 1.2 同上：异常轮仍 `state.Counter = n`（本轮模块已 LoadBytes 进 VM，重用轮号会让旧抛出 `Eval{n}`
      "粘住"、后续输入全部复现该异常）；`ExtendWorld` 移到 Invoke 成功之后（失败轮不污染编译世界）。
- [x] 1.3 文件头注释更新（编译失败**或运行异常**均不推进会话、作失败返回、异常不逃逸）。
- [x] 2.1 回归测试 `tests/repl_eval_exception/`（driver.z42 + expected_output.txt）：throw / 除零被捕获、
      会话变量在异常后完好、异常后仍可新声明并求值。
- [x] 2.2 README 同步（功能索引 Script.z42 行 + 关联文档 change 登记）。
- [x] 3.1 本地 e2e 实证：z42i 混合异常+有效表达式，进程不退出、会话不粘（✅ 已验，见备注）+ driver app 对账。
- [x] 3.2 GREEN（CI 全绿，PR #352 已合并 `5317477a`）：`cargo build --release`（✅ afe9edca runtime 已建）+ `xtask test` 全 stage——
      **本机 workspace/并行构建 z42vm 死锁挂起（已知本机条件，见 memory runtime-review-improvement-program）**，
      单库构建正常（已用于验证）。纯附加 stdlib 错误处理、不触 runtime 代码 → **以 CI 全绿为准**（PR 门禁）。

## 备注
- **实证验证（已完成）**：patched z42i 跑 `int a=10 / a / throw / a / int b=20 / a+b / 1/0 / a+b`
  → 逐条输出 `10 / 10 / Exception: boom / 10 / 20 / 30 / DivideByZeroException: integer / by zero / 30`，
  exit=0（仅 `.exit` 退出）。driver.z42 编成 app 单跑输出与 expected_output.txt 逐行一致。
- **发现（超出本 fix scope，供 User）**：z42.scripting/tests/ 下 10 个 `driver.z42`+`expected_output.txt`
  测试**未被任何自动 gate 发现**（e2e / lib-units 均只扫 `source.z42`）——它们是**手动验证产物**（#343
  亦手动验）。本 fix 沿用该既有约定；是否把这些 repl driver 测试接入自动 gate 建议另立 change。
