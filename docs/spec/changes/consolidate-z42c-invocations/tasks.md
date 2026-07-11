# Tasks: consolidate-z42c-invocations（+ JIT 编译加速）

> 状态：🟢 A 步已落地并验证（2026-07-12）| B 步待前置 | 创建：2026-07-11 | 类型：refactor（A）+ chore（B）
> toolchain 锁：A 步在锁释放后（fix-bootstrap-format-bump-deadlock 归档）落地；A 是完整行为保持
> refactor，B（翻 jit）是未来一行改动、待前置齐再作独立 change 重登记锁。

## A 步：收敛（默认 interp，纯行为保持）✅ 已落地并验证

- [x] A1 `common/xtask_common.z42`：加 `_z42cMode()`（读 `Z42C_BUILD_MODE`，默认 `"interp"`）。
- [x] A2 加 `_z42cWorkspaceBuild(vm, driver, cwd, libs)`（内部 `_z42cMode()`；统一 stdout/verbosity）。
- [x] A3 3 处 workspace 构建改调 A2：stdlib / compiler gen1 / gen2。核对同参，不变量现由代码保证。
- [x] A4 14 处裸调用点 `.Arg("--mode").Arg("interp")` → `.Arg("--mode").Arg(_z42cMode())`（sed 逐文件）。
- [x] A5 **例外**：`xtask_bootstrap_check.z42:145` 保持 interp（D4）——已确认仅此 1 处残留 interp。
- [x] A6 重建 xtask.zpkg 42/42 + **`test compiler` 不动点 7/7 gen1==gen2 逐字节 + 19 units + e2e 全绿**
      （默认 interp，行为保持验证通过）。
- [x] A7 smoke：`bench --quick`（走 `_z42cMode()` 默认 interp）exit 0、有序选集 01/02。
      （`Z42C_BUILD_MODE=jit` 全平台不动点验证归 `jit-fixpoint-check.yml` + B 步。）
- [x] A8 A 步提交（本 change 保持开放待 B；见下）。

## B 步：翻默认到 jit — 前置齐后做

- [ ] B0 **前置核对**（全绿才开）：① toolchain 锁在手 ② `jit-fixpoint-check.yml` 全平台绿
      ③ User 拍板前置#2 ④ 非格式-bump 周期。
- [ ] B1 `_z42cMode()` 默认 `"interp"` → `"jit"`（一行）。
- [ ] B2 全 GREEN gate 在 jit 下绿 + 不动点 7/7；CI 全平台绿几轮观察。
- [ ] B3 `docs/book/src/dev/build.md` + `xtask.md` 记：z42c 编译默认 jit + `Z42C_BUILD_MODE=interp`
      逃生舱 + 附录 A 证据链接。
- [ ] B4 归档 B（含 `docs/xtask_review.md` 附录 A 标「已落地」）。

## 备注
- A 与 B 分两个 commit（甚至两次归档）；A 是安全 refactor，B 是需前置的开关翻转。
- 逃生舱 `Z42C_BUILD_MODE=interp` 在格式-bump 窗口 / 确定性审计 / 调试时随时可回 interp。
