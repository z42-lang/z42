# Tasks: cross-zpkg gate 按 app 名选 zpkg——根治 flaky "no entry point"

> 状态：✅ 完成 | 创建：2026-07-25 | 分支：fix/crosszpkg-run-app-by-name（隔离 worktree z42-xfix）
> 类型：fix（toolchain harness）

**变更说明：** cross-zpkg 测试 gate（`test-cross-zpkg` + `test-consume` 两 CI job 共用）间歇性
报 `no entry point` → 根治：Stage 4 改按**主应用项目名**选 zpkg，而非 `_firstZpkg`（sorted glob[0]）。

**根因（已确认，主线问题非某 PR）：** `generic_field_carry`（#30 引入）主应用 `demo.gmain`（`pack=true`）
依赖 `demo.gext`，而 `demo.gext` 字母序**排在 `demo.gmain` 前**。packed exe build 有时把依赖 zpkg
落到 main 的 dist（间歇泄漏），`_firstZpkg` 的 sorted glob[0] 就选中 `demo.gext.zpkg`（库、无烘焙
入口）→ `no entry point`。**唯一受害** = 该 case（其余 case 主应用名都排最前）；main CI 该腿
failure/success 混着（flaky）印证间歇泄漏。

**修复：** `xtask_test_cross.z42` Stage 4：新增 `_tomlName(mainToml)`（读 [project] `name`）→ 找
`<appName>.zpkg`（`File.Exists` 校验）；未命中回退旧 `_firstZpkg`。运行的永远是**所建的那个 app**，
与 dist 里还有什么无关。纯 harness 改动、无格式 bump、无 compiler/runtime 改动。

- [x] 1.1 `scripts/test/xtask_test_cross.z42`：`_tomlName` + Stage 4 按 app 名选 zpkg（回退 _firstZpkg）
- [x] 1.2 验证：cross-zpkg 全 6 例通过（含 generic_field_carry，且 main dist 内确有 demo.gext 排前）；手工反证：直接跑 sorted-first `demo.gext.zpkg`→`no entry point`（旧行为），跑 `demo.gmain.zpkg`→输出 0（fix 行为）
- [x] 1.3 归档 + PR（本地验，CI 为最终权威——flaky 复现需多跑）

## 备注
- xtask.zpkg 由 harness 源编译 → 验证需重建 xtask（本地 bootstrap 已含）。
- 泄漏本身（packed build 间歇把 dep 落 dist）是次要/cosmetic——按名选 app 令其对正确性无影响，属正确设计（跑你建的 app，而非"目录里第一个 zpkg"）。
