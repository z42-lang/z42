# Proposal: 收敛 z42c 调用 + 为 JIT 编译加速铺路

> 状态：🟡 DRAFT / **排队中**（toolchain 子系统被 `fix-bootstrap-format-bump-deadlock` 占用；
> 锁释放后登记并实施）。类型：refactor（A 步纯行为保持）+ chore（B 步翻默认）。
> 证据与前置见 `docs/xtask_review.md` 附录 A。

## Why

1. **18 处 z42c driver 调用点散落 12 文件、全硬编码 `.Arg("--mode").Arg("interp")`**——
   review §2.1 已指出其中 3 处 `--workspace` 构建（stdlib gen / compiler gen1 / gen2）几乎逐字节
   相同，且「gen1、gen2 必须完全同参」的自举不动点不变量**只靠注释维持**（2026-07-05 gen2 漂移
   bug 正是分歧造成）。散落 = 漂移温床 + 无法一处切换执行模式。
2. **z42c 走 JIT 实测比 interp 快 3.6×（大编译）/ 1.67×（小编译），产物 byte-identical**
   （附录 A）。但 18 处硬编码 interp 使这个加速**无法一处启用**；且切换需要一个「默认 jit /
   可回退 interp」的开关，用于格式-bump 窗口 / 确定性审计 / 调试。

两件事同源——**先把调用收敛进带 mode 参数的 helper（把不变量从注释升级为代码 + 造出统一开关），
再翻默认到 jit** 是最自然的落地路径。

## What Changes

**A 步（refactor，纯行为保持，默认仍 interp）**：

- 新增 `_z42cMode()`（`common/xtask_common.z42`）：读 `Z42C_BUILD_MODE` 环境变量，
  **A 步默认返回 `"interp"`**（零行为变更），接受 `interp` / `jit`。
- 新增 `_z42cWorkspaceBuild(vm, driver, cwd, libs)`（§2.1）：封装 3 处 `--workspace --release`
  构建，内部用 `_z42cMode()`；**把「gen1/gen2 同参」不变量固化成代码**。stdlib:91 / compiler:70 /
  compiler:308 三处改调它。
- 其余 15 处 `.Arg("--mode").Arg("interp")` → `.Arg("--mode").Arg(_z42cMode())`（或经 helper）。
- **净效果：默认行为逐字节不变（仍 interp），但执行模式收敛到单一开关。**

**B 步（chore，一行翻默认，前置齐后做）**：

- `_z42cMode()` 默认 `"interp"` → `"jit"`。全部 z42c 编译改走 JIT（≈3.6×/1.67× 加速）；
  `Z42C_BUILD_MODE=interp` 作逃生舱。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `scripts/common/xtask_common.z42` | MODIFY | 新增 `_z42cMode()`（+ 可选 `_z42cWorkspaceBuild` 放这或 build/xtask_toolchain.z42） |
| `scripts/build/xtask_stdlib.z42` | MODIFY | workspace 构建(:91) → `_z42cWorkspaceBuild` |
| `scripts/build/xtask_compiler.z42` | MODIFY | 5 处：gen1(:70)/gen2(:308)→helper；其余 3 处 → `_z42cMode()` |
| `scripts/build/xtask_bootstrap_check.z42` | MODIFY | 1 处（**决策点**：见 design——边界检查是否随 jit，或钉 interp 作保守 gate） |
| `scripts/build/xtask_test_assets.z42` | MODIFY | golden 编译(1) → `_z42cMode()` |
| `scripts/test/xtask_test_lib.z42` | MODIFY | 1 |
| `scripts/test/xtask_test_lib_units.z42` | MODIFY | 2 |
| `scripts/test/xtask_test_cross.z42` | MODIFY | 1 |
| `scripts/test/xtask_test_platform.z42` | MODIFY | 1 |
| `scripts/test/xtask_test_incremental.z42` | MODIFY | 1 |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | 2 |
| `scripts/xtask_bench.z42` | MODIFY | 1 |
| `scripts/install/xtask_install_vscode.z42` | MODIFY | 1 |
| `docs/book/src/dev/build.md` / `xtask.md` | MODIFY | 记 `Z42C_BUILD_MODE` 开关 + 默认 jit（B 步落地时） |

**只读引用**：`docs/xtask_review.md` 附录 A（证据）、`src/runtime/src/main.rs`（默认 mode 逻辑）。

## Out of Scope

- 不改 z42vm 自身的默认 mode（已是 jit，`make-jit-default`）。
- 不动 golden 运行侧 `test e2e --mode jit`（那是跑 golden 的模式，非 z42c 编译模式）。
- B 步在前置未齐前不做（见下）。

## 前置（B 步翻默认前必须逐项达成）

1. **toolchain 锁释放**（当前 `fix-bootstrap-format-bump-deadlock` 占用）——A、B 都要。
2. **`.github/workflows/jit-fixpoint-check.yml`（已加）全平台绿**：linux-x64/arm64/windows 的
   z42c 7 包 interp==jit byte-identical（macos-arm64 本地已验）。
3. **User 拍板前置#2**：接受「编译信任基线从 interp 移到 cranelift/JIT」（附录 A #2，设计决定）。
4. **格式稳定期**：不踩在 zbc/zpkg format bump 同周期（避免红了分不清 JIT 还是格式）。

## Open Questions

- [ ] bootstrap-check（`_bcRunWorkspace`）是否随 jit？倾向**钉 interp**（保守 gate，nightly z42c 跨
      平台 jit 成熟度未验），design 定。
- [ ] fixpoint（test compiler gen1/gen2）：两代同走 `_z42cMode()`（默认翻 jit 后两代都 jit，不变量
      仍成立）；「interp 参考」性由 jit-fixpoint-check 工作流 + `Z42C_BUILD_MODE=interp` 逃生舱保证。
