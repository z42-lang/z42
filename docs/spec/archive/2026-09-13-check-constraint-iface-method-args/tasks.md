# Tasks: check-constraint-iface-method-args

> 🔴=未开始 🟡=进行中 🟢=完成 ｜ proposal/design 见同目录

## 阶段 0（先量爆炸半径，再写实现）

- 🟢 T0 与实现合并直接发真 E0463 测量：用新 z42c `build stdlib` + `build compiler` 全量，
  **E0463 命中 = 0**（stdlib 0 / compiler 0），无其它新错误。⇒ 零假阳性、零现存真欠债、build 源零触发
  ⇒ 零字节漂移由构造保证。DRAFT 估算坐实。

## 实现

- 🟢 T1 DiagnosticCodes 加 `ConstraintMethodArgTypeParam = "E0463"` + 注释（语义层用字面量 "E0463" 发码，D6）。
- 🟢 T2 新检查 `MemberResolver._checkConstraintIfaceArgs` + `_checkOpaqueParamArg`（D3 判据 + D4
  `_containsSelf` 门控），注入 `MemberResolver:218` `BindArgsToSignature` 之后。`_containsSelf` 保持
  private static（同类可访问，未提 internal）；检查贴着唯一调用点、不污染 OverloadBinder。
- 🟢 T3 params 尾位同款门控（防御性）已含在 `_checkConstraintIfaceArgs`。

## 测试

- 🟢 T4 编译期负例门（`constraint_member_tests.z42`，`bodyDiags` harness，扩现有文件）：`a.Same("nope")`
  → 1×E0463 / `a.Same(b:P)` 具体实现类 → 1×E0463 / class-level `this.It.Same("nope")` → 1×E0463。
  **退回对照坐实**：只注释掉 `_checkConstraintIfaceArgs` 调用 + 重建 compiler → **恰这 3 条 FAIL**
  （371 passed, 3 failed），其余全绿 ⇒ 真门、判据精确。
- 🟢 T5 无误伤对照：既有 `test_self_param_with_same_type_param_reports_nothing`（`T`-to-`T`）+
  `test_unconstrained_type_param_receiver_reports_nothing` 仍 0 诊断（我的检查对含型参实参 / 无约束收者
  天然跳过）。`test compiler` 全 0 failed。
- 🟢 T6 **不新增运行期 e2e（scope 裁决）**：本 change **零发射变化**（纯编译期诊断）⇒ 正确路径运行期
  行为由现有全套 stdlib/e2e GREEN 完整覆盖（PriorityQueue/SortedSet 的约束方法都在跑）；负例编不过
  无法跑。同 #548（纯诊断 change）的既定模式：编译期门 + 退回对照 + 完整 GREEN 即足，运行期门留给
  改发射的 change（如 #563）。

## GREEN + 落地

- 🟢 T7 完整 GREEN：`./xtask test` **全 stage 绿** + 自举不动点 **3/3 gen1==gen2**（lines 过，
  MemberResolver 845 行）；`test stdlib --mode jit` ✔（332 文件）；`test e2e --dir cross-zpkg --mode jit`
  ✔（28 过）；`test bootstrap` ✅（nightly z42c 编当前源无边界越界、repo 自建 OK，无格式 bump）。
- 🟢 T8 文档同步：`generic-constraints.md` 的「已知限制：形参本身是型参时仍不检查」节改写为
  「`Self` 形参位：具体类型实参报 E0463」+ 与 E0454 的分工 + Deferred 剩余边界说明。
- 🟢 T9 归档 changes→archive + 本 tasks 改 🟢；PR（合并前 rebase origin/main + 重跑 GREEN；lang 变更
  等 User sign-off 再 merge，合并后删分支/worktree）。

## 环境（已就绪）

- worktree `../z42-constraint-args`（branch `check-constraint-iface-method-args`，基于 origin/main
  `ccee746b`）；nightly SDK 冷种子已供（`da3694a7`=main−2，零格式 skew），×2 收敛完成、dist 非零。
- 每条命令带 `Z42_PORTABLE_VM=$PWD/.seedvm/z42vm`。退回对照基线在 `scratchpad/sdk/`（修前成套）。
