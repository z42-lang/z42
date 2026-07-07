# Design: 收敛 xtask deps（三正交子命令 + 依赖两层模型）

## Architecture

```
命令面（用户触发）                         内部（自动，无命令面）
─────────────────────────                ─────────────────────────
deps check [--os <p>]                    test platform android run
  ├─ presence: _setup*(mode="check")       └─ emulator/AVD 缺 → _depsInstallAndroidEmulator
  └─ drift:    单实现（regex 版）         test platform wasm run
deps install [--os <p>] [--force]          └─ hermetic+PATH node 均缺 → _depsInstallNode
  ├─ 跨平台: rust/node 版本告警
  ├─ android: rust targets + cargo-ndk + JDK + SDK(build tier)
  ├─ ios:     rust targets + Xcode 检查
  └─ wasm:    rust targets + wasm-pack + node（新划入必备）
deps env [--os android]
  └─ export ANDROID_NDK_HOME=…（原 --print-env）
```

依赖两层模型（User 裁决 2026-07-07）：

| 层 | 判据 | 安装时机 |
|----|------|---------|
| **平台必备** | 该平台的构建/测试没有它就跑不了（rust targets、cargo-ndk、SDK build 包、wasm-pack、**node（wasm）**） | `deps install --os <p>` 直接装 |
| **用到才装** | 只有特定步骤需要、体积重或纯运行期（emulator+AVD+Gradle ~4GB、node 兜底） | 消费步骤检测缺失 → 自动安装，打日志，不需要用户触发 |

## Decisions

### Decision 1: drift 单实现保留 regex 版（`xtask_deps.z42`），删手写扫描版

**问题：** 两套 drift 实现留哪套。
**选项：** A — 留 `_checkVersionsDrift`（Regex `_firstMatchGroup`，覆盖面更全：含
Cargo.toml workspace version + wasm 段）；B — 留 `_checkAndroidDrift`/`_checkIosDrift`
（手写 `_firstIntAfter`，支持 `--os` 过滤）。
**决定：** 选 A 为基底，把 B 的 `--os` 过滤能力并入（按平台分段执行），删 B 全部
（含 `_firstIntAfter`）。regex 锚点解析更健壮（历史上 iOS 锚点迁移过一次，regex 版
有完整注释记录），且 A 已是 `deps check` 的实现——调用点零迁移。

### Decision 2a: presence 退出码策略——drift 恒致败，presence 仅 `--os` 时致败（实施期修正 2026-07-07）

**事实修正：** 起草时称"CI 不使用这些命令"，实施时发现 build-and-test 在无平台 SDK
的 runner 上**裸跑 `deps check`** 当 drift 门禁（ci.yml:131-138）。presence 若计入
裸跑退出码，CI 全红。
**决定：** drift 与机器无关 → 恒致败；presence 仅显式 `--os <p>`（调用者声明开发
平台 p）时致败，裸跑时信息性展示 + note 提示。这同时更符合本地语义——不是每个人
都开发全部平台。CI 零 workflow 改动。

### Decision 2: presence 检查并入 `deps check`，复用 `_setup*(mode="check")`

`install --check` 的存在性检查逻辑不重写：`_setupAndroid`/`_setupIos`/`_setupWasm`
已支持 `mode == "check"` 分支，`deps check` 直接以 check 模式调用它们 + 跨平台
`_checkRust`/`_checkNode`，再跑 drift 段。install 侧删掉 mode 参数里的 "check"/"drift"/
"print-env" 取值后，`_setup*` 的 mode 退化为 `install|check` 两值（check 供 deps check 用）。

### Decision 3: node 划入 wasm 平台必备

**事实：** wasm 三阶段测试的 ③ RunTests 用 local node + Playwright（`xtask_test_wasm.z42:41-46`
优先 hermetic `artifacts/tools/node`，CI fallback PATH）——node 不在，wasm 平台的测试
面就不完整。
**决定：** `_setupWasm` 在 rust targets + wasm-pack 后追加 `_depsInstallNode(force)`
（幂等，已装即跳过）；`deps check --os wasm` 相应检查 hermetic/PATH node ≥ min 版本。
Windows 维持现状（安装器 POSIX-only，打印 MSI 指引）。

### Decision 4: emulator 转"用到才装"，钩在 AndroidBackend.RunTests

**事实：** `--os android`（build tier）与 `android-emulator` step 共用
`_depsInstallAndroidSdk(force, tier)`，差异只是 tier 集合（emulator tier = build 集
PLUS emulator + system-image + AVD + Gradle，~4GB / 10-15 min）。emulator 只有
`test platform android run` 需要；打包/构建腿不需要。
**决定：** 不并入 `--os android` 直接装（会让所有构建场景背 4GB）；改为 RunTests
桥接 test.sh 之前探测 emulator/AVD 目录，缺 → 打印「installing android emulator
tier (~4GB, one-time)」→ `_depsInstallAndroidEmulator(false)`。
**emulator 零命令面（User 裁决 2026-07-07：不提供命令，彻底隐藏）**——
`_depsInstallAndroidEmulator` 纯内部函数，任何 xtask 子命令/flag/positional 都不
暴露它；对用户可见的只有 `deps check --os android` 的状态行（✗ 时注明「will
auto-install on `test platform android run`」）和自动安装时的日志。

### Decision 5: `deps env` 独立子命令

`--print-env` 的用法是 `eval "$(z42 xtask.zpkg deps install --os android --print-env)"`
——需要纯净 stdout，不能混进 check/install 的进度输出。独立 `deps env [--os android]`
最干净；ios/wasm 无 env 可导出，仅 android 生效（无 `--os` 时默认 android）。

### Decision 6: 与后续 change 的衔接

- `add-vscode-syntax-ext`（排队）：在收敛后的 `install` 上加 optional positional
  component（`deps install vscode`）。本变更删掉 step positional 后，positional 槽
  留空，由该变更引入语义（编辑器资产 = 用户显式触发，不属于两层模型的任何一层，
  是第三类：**主机集成，装不装由用户决定**）。
- `simplify-xtask-verify`（toolchain 锁持有者，先行）：收敛 test/verify 面；本变更
  收敛 deps 面，互不重叠（该 change 动 test/build 分发，本 change 动 deps 分发）。
  实施顺序：verify 归档 → 本变更接锁 → vscode。

## Implementation Notes

- **ArgParser 行为**：删 flag 后旧用法（`--drift` 等）由 Std.Cli 自然报 unknown
  option，无需兼容 shim（pre-1.0 不留旧路径）。
- **自动安装的失败语义**：lazy 安装失败 → RunTests 直接失败并透出安装器错误
  （不吞、不降级跳过），与「测试基础设施缺失 = 测试失败」一致。
- **幂等**：`_depsInstallNode` / `_depsInstallAndroidSdk` 均已幂等（版本匹配即跳过），
  lazy 钩子无需额外去重。
- **xtask 源 API 面**：全部改动只用现有 stdlib API（Std.Cli/IO/Toml/Regex + Process），
  无自举 support/use 分期问题。

## Testing Strategy

- 变更类型 toolchain（对外 CLI 行为变更）：
  - smoke：`deps check`（全平台 + `--os` 各值）、`deps install --os wasm`（node 装入
    hermetic 目录）、`deps env`（stdout 纯净可 eval）——结果记 tasks.md 备注
  - 负路径：旧 flag `--drift`/`--check`/`--print-env`、旧 step `node` → ArgParser 报错非 0
  - wasm 真校验：临时改 versions.toml min 值 → check 报 ✗（验后还原）
- lazy 钩子：本地无 emulator 状态下 `test platform android run` 触发自动安装（或以
  目录改名模拟缺失）；wasm 同理
- GREEN gate：裸 `xtask test` 全绿（deps 不在 gate 链，回归风险集中在 xtask 编译自身
  ——build wave 编 xtask 即覆盖）
