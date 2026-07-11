# Tasks: consolidate-z42c-invocations（+ JIT 编译加速）

> 状态：🟢 已完成 | 完成：2026-07-12 | 创建：2026-07-11 | 类型：refactor（A）+ chore（B）
> toolchain 锁：A 步在锁释放后（fix-bootstrap-format-bump-deadlock 归档）落地；B 步 2026-07-12
> 前置全齐后占 toolchain 锁翻默认 jit（一行 + docs）。归档时释放 toolchain 锁。

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

- [x] B0 **前置核对**（全绿才开）：① ✅ toolchain 锁在手（fix-bootstrap 归档时释放、明指解锁本 change）
      ② ✅ `jit-fixpoint-check.yml` **全平台绿**（run 29168922905，2026-07-12：linux-x64 / linux-arm64 /
      windows-x64 / macos-arm64 四平台 z42c workspace 编译 interp==jit 逐字节一致）③ ✅ User 拍板前置#2
      （信任基线从 interp 移到 cranelift/JIT）④ ✅ 非格式-bump 周期（runtime 锁空闲、末次 bump 0.31 已归档）。
- [x] B1 `_z42cMode()` 默认 `"interp"` → `"jit"`（一行 + 说明注释；`scripts/common/xtask_common.z42`）。
- [x] B2 本地 `test compiler`（jit 默认）不动点 **7/7 gen1==gen2** + 19 units + e2e 全绿。全量 gate-under-jit
      交 clean CI（共享树含他 session stdlib WIP，本地全量不可靠）——**CI 全平台绿几轮观察后归档**。
- [x] B3 `docs/book/src/dev/build.md`「z42c 编译执行模式」节记默认 jit + `Z42C_BUILD_MODE=interp` 逃生舱 +
      附录 A 证据链接（bootstrap-check 恒 interp 例外注明）。
- [x] B4 归档 B（`docs/xtask_review.md` 附录 A 已标「已落地」）。B 提交 1af93562 的 **CI 全平台全量
      gate-under-jit 绿**（compile-toolchain linux-x64/macos-arm64 等全 success，无失败 job）；jit 现为默认，
      此后每次 main CI 自动续观察。证据充分故归档。

## 备注
- A 与 B 分两个 commit（甚至两次归档）；A 是安全 refactor，B 是需前置的开关翻转。
- 逃生舱 `Z42C_BUILD_MODE=interp` 在格式-bump 窗口 / 确定性审计 / 调试时随时可回 interp。
