# Spec: 运行时旋钮的构建期校验

## ADDED Requirements

### Requirement: 未知旋钮名在构建期被报出

#### Scenario: `[profile.*.runtime]` 有 typo
- **WHEN** 工程 manifest 的 `[profile.release.runtime]` 含 `gc-mdoe = "stw"`，执行 `z42c build`
- **THEN** stderr 出现
  `z42c build: warning: [profile.release.runtime] 未知运行时旋钮 \`gc-mdoe\`——是不是 \`gc-mode\`？`
  并附一行"它仍会被烤进侧车 / `z42vm --list-knobs --all`"
- **AND** build **成功**（exit 0），侧车照常产出

#### Scenario: 找不到足够近的已知旋钮
- **WHEN** 键名与任何已知旋钮的编辑距离都超过阈值（≤3 且 ≤ 键长一半、下限 1）
- **THEN** 只报"未知运行时旋钮 \`<key>\`。"，不给建议

#### Scenario: 旋钮名全部正确
- **WHEN** `[profile.release.runtime]` 只含登记表内的键（如 `mode` / `gc-mode`）
- **THEN** 无任何旋钮相关输出（构建期零噪音）

#### Scenario: 元旋钮写进配置文件
- **WHEN** `[profile.release.runtime]` 含 `Z42_CONFIG = "x.toml"`
- **THEN** 按未知旋钮名报 warning —— 元旋钮没有配置文件 key 形式，与运行时文件层的判定一致

#### Scenario: 只构建 debug，但 release profile 有 typo
- **WHEN** 执行不带 `--release` 的 `z42c build`，而 typo 在 `[profile.release.runtime]`
- **THEN** 仍报 warning —— 校验遍历全部 profile，与本次构建的是哪个无关

#### Scenario: 库工程
- **WHEN** 工程 `kind = "lib"`（不产侧车）且 `[profile.*.runtime]` 有 typo
- **THEN** 仍报 warning —— 校验不挂在侧车写出路径上

### Requirement: 构建期不判定可用性

#### Scenario: 已知但本机 build 不支持的旋钮
- **WHEN** `[profile.release.runtime]` 含 `jit-profile = "1"`，而构建机的 z42vm 带 jit feature、
  目标机是 interp-only 构建
- **THEN** 构建期**不报**任何东西 —— 可用性只有目标机的运行时有真答案

## MODIFIED Requirements

### Requirement: `[profile.<n>]` 下直接写键被拒

**Before**：检查在 `_writeRuntimeConfigSidecar` 内，只在 `isExe`（且走到写侧车那步）时才跑；
库工程的 `[profile.X]` 直接写键**不报错**，且 exe 工程也要等到编译完成后才报。

**After**：检查前移到 `_build` 早期（`_validateProfileKnobs`），**编译前** fail fast，
且对 `kind = "lib"` 一视同仁。文案与退出码不变（`ExitCode.BuildError`）。

#### Scenario: 库工程写了旧形状
- **WHEN** `kind = "lib"` 的工程含 `[profile.release]` 且其下直接写 `mode = "interp"`
- **THEN** 报 `z42c build: [profile.release] 不接受直接写键（\`mode\`）——` 并指出该写进
  `.runtime` / `.properties`，**exit != 0**，**不进入编译**
