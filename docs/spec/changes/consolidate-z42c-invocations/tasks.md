# Tasks: consolidate-z42c-invocations（+ JIT 编译加速）

> 状态：🟡 DRAFT / 排队中（toolchain 锁被 `fix-bootstrap-format-bump-deadlock` 占用）
> 创建：2026-07-11 | 类型：refactor（A）+ chore（B）

**开工前**：toolchain 锁释放 → 登记 ACTIVE.md toolchain 行 → 走阶段 6.5 确认（A 可直接；B 需前置齐）。

## A 步：收敛（默认 interp，纯行为保持）— 锁释放后随时可做

- [ ] A1 `common/xtask_common.z42`：加 `_z42cMode()`（读 `Z42C_BUILD_MODE`，默认 `"interp"`）。
- [ ] A2 加 `_z42cWorkspaceBuild(vm, driver, cwd, libs)`（内部 `_z42cMode()`；统一 stdout/verbosity）。
- [ ] A3 3 处 workspace 构建改调 A2：`xtask_stdlib.z42:91` / `xtask_compiler.z42:70`（gen1）/ `:308`（gen2）。
      核对 gen1/gen2 原本同参（simplify-compiler-build 教训），并入后不变量由代码保证。
- [ ] A4 其余 14 处裸调用点 `.Arg("--mode").Arg("interp")` → `.Arg("--mode").Arg(_z42cMode())`：
      compiler(3 处非 workspace) / test_assets / test_lib / test_lib_units(2) / test_cross /
      test_platform / test_incremental / package_desktop(2) / bench / install_vscode。
- [ ] A5 **例外**：`xtask_bootstrap_check.z42` 保持 interp（D4，不走 `_z42cMode()`）。
- [ ] A6 重建 xtask.zpkg 41/41 + `test compiler` 不动点 7/7（默认 interp，逐字节等价当前）+ 全 GREEN gate。
- [ ] A7 smoke 开关：`Z42C_BUILD_MODE=jit xtask build compiler && xtask test compiler` 不动点仍 7/7。
- [ ] A8 归档 A。

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
