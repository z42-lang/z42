# Design: 收敛 z42c 调用 + JIT 编译加速

## Architecture

```
                       ┌─ Z42C_BUILD_MODE (env) ─┐
                       │  A 步默认 "interp"       │  ← 单一开关
                       │  B 步默认 "jit"          │
                       └───────────┬─────────────┘
                                   ▼
                          _z42cMode()  (common/xtask_common.z42)
                                   ▼
         ┌─────────────────────────┼──────────────────────────┐
         ▼                         ▼                           ▼
 _z42cWorkspaceBuild        _z42cBuildToml(已存在)      裸调用点 .Arg("--mode").Arg(_z42cMode())
 (3 处 --workspace，§2.1)    (单 toml，package 用)        (golden emit / test lib / bench / …)
   gen1/gen2 同参不变量           mode 参数化                  15 处
   固化成代码
```

## Decisions

### D1：mode 开关放环境变量 `Z42C_BUILD_MODE`，不放 CLI flag
**问题**：怎么让 18 处统一切 interp↔jit + 留逃生舱？
**选项**：A CLI flag（每命令加 `--z42c-mode`）；B 环境变量单一读取点。
**决定**：**B**。18 处调用分散在 build/test/package/bench 各命令，加 CLI flag 要每个命令面都改 +
透传；环境变量 `_z42cMode()` 一处读、子进程无关，逃生舱 `Z42C_BUILD_MODE=interp ./xtask …` 天然可用。
默认值内建（A=interp / B=jit），env 覆盖。

### D2：A 步默认钉 interp（纯行为保持），B 步单独翻 jit
**问题**：收敛和切换要不要一次做？
**决定**：**分两步**。A（收敛 + mode 参数化，默认 interp）**逐字节零行为变更**，锁一释放随时能落、
不依赖任何 jit 前置或 User 决定；B（翻默认到 jit）是**一行改动**，等前置#2/#3/工作流全绿再做。
好处：§2.1 的不变量固化价值立刻兑现，jit 切换的风险面单独隔离到一行。

### D3：gen1/gen2 不变量固化进 `_z42cWorkspaceBuild`
**问题**：review §2.1 —— gen1(:70)/gen2(:308) 必须完全同参，只靠注释。
**决定**：抽 `_z42cWorkspaceBuild(vm, driver, cwd, libs)`，两代都调它 → 同参从**结构**上成立，
注释纪律升级为代码保证。mode 也走 `_z42cMode()`，两代恒同 mode（翻 jit 后两代都 jit，不动点
gen1==gen2 仍成立）。

### D4：bootstrap-check 钉 interp（保守 gate）
**问题**：`_bcRunWorkspace`（跨版本自举边界检查）随 jit 还是钉 interp？
**决定**：**钉 interp**（不走 `_z42cMode()`，或传显式 interp）。理由：① 它是**正确性 gate**（"上一版
nightly 能否编当前源"），非速度敏感路径；② 它跑**下载的 nightly z42c**，其 jit 在各平台的成熟度未
经 jit-fixpoint-check 覆盖；③ 保守起见让边界检查恒定在最被信任的 interp。速度损失可忽略（该路径
每次改编译器才跑）。

## Implementation Notes

### `_z42cMode()`（common/xtask_common.z42，新增）
```z42
// z42c 编译执行模式的单一开关。A 步默认 "interp"（行为保持）；B 步翻 "jit"（≈3.6×/1.67× 加速，
// 见 docs/xtask_review.md 附录 A，interp==jit byte-identical 已验）。Z42C_BUILD_MODE=interp 逃生舱。
string _z42cMode() {
    string m = Environment.GetEnvironmentVariable("Z42C_BUILD_MODE") ?? "";
    if (m == "interp" || m == "jit") { return m; }
    return "interp";   // ← B 步改成 "jit"
}
```

### `_z42cWorkspaceBuild(vm, driver, cwd, libs)`（§2.1，新增；放 common 或 build/xtask_toolchain.z42）
封装：
```z42
int rc = new Process(vm).Arg(driver).Arg("--mode").Arg(_z42cMode()).Arg("--")
    .Arg("build").Arg("--workspace").Arg("--release")
    .WorkingDirectory(cwd).Env("Z42_LIBS", libs)
    .Stdout(_verbAtLeast(3) ? Stdio.Inherit() : Stdio.Null()).Stderr(Stdio.Inherit()).Run().ExitCode;
```
调用点：`xtask_stdlib.z42:91`（cwd=src/libraries）、`xtask_compiler.z42:70`（gen1，cwd=src/compiler）、
`xtask_compiler.z42:308`（gen2，cwd=src/compiler）。**注意**：三处原本的 stdout/stderr/verbosity
处理要核对一致后并入 helper（review §2.7 提过 verbosity 用法不一，一并统一）。

### 15 处裸调用点
`.Arg("--mode").Arg("interp")` → `.Arg("--mode").Arg(_z42cMode())`。逐一见 proposal Scope 表。
（bootstrap-check 那 1 处**例外**，D4，保持 interp。）

## Testing Strategy

**A 步（默认 interp，行为保持）**：
- xtask.zpkg 重编 41/41 通过。
- `test compiler` 不动点 7/7 gen1==gen2 byte-identical（验 `_z42cWorkspaceBuild` 收敛无回归）。
- 全 GREEN gate（`xtask test`）—— A 步默认 interp，应逐字节等价于当前。
- smoke：`Z42C_BUILD_MODE=jit xtask build compiler` + `test compiler` 不动点仍 7/7（验开关 + jit 路径）。

**B 步（翻 jit）**：
- 前置：`jit-fixpoint-check.yml` 全平台绿 + User 拍板 + 锁释放 + 格式稳定期。
- 翻默认后：全 GREEN gate 在 jit 下绿 + 不动点 7/7 + CI 全平台绿几轮。
- 回退：`Z42C_BUILD_MODE=interp` 或 revert 一行。

## Rollback

- A 步：纯 refactor，revert 即回。
- B 步：**一行**（`_z42cMode()` 默认 jit→interp）或 `Z42C_BUILD_MODE=interp` 环境覆盖即时回退——
  格式-bump 窗口或发现 jit 分歧时的逃生舱。

## 证据（docs/xtask_review.md 附录 A）

- 速度：build z42.core interp 18.65s → jit 5.22s（3.6×）；emit 小文件 2.56s → 1.53s（1.67×）。
- 确定性：z42c 7 包 macos-arm64 interp==jit 全 byte-identical（除 BLID）；z42.core + 小文件亦然。
- 跨平台：linux/windows 由 `jit-fixpoint-check.yml`（workflow_dispatch）补。
