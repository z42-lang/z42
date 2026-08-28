# Proposal: z42.scripting 物理搬入 src/libraries（成 stdlib 成员）

> 类型：`refactor`（物理搬迁 + build/workspace 重接，不改外部行为）。REPL/eval 行为逐字节等价。
> 归属：`stdlib-interop-and-repl-split-program` 轴 2 的 **follow-up**（继 PR1 `split-z42-repl` /
> PR-A `converge-z42-syntax-lib` / PR-B `sink-repl-compile-facade`）。

## Why

PR-B（`sink-repl-compile-facade`）后 `z42.scripting` 编译期已 **stdlib-only**——编译走 `z42.build` 的
`IReplCompiler` 门面（实现 `Z42cReplCompiler` 住 `z42c.pipeline`，运行期反射注入），前端 `z42c.core/syntax`
已由 PR-A 下沉 `src/libraries/`。但 scripting **物理仍在 `src/toolchain/`**，由 `_buildScriptingLib`（一个
「stdlib + z42c 合并 Z42_LIBS」的特例步）构建——这个特例步在 scripting 变 stdlib-only 后已无必要。

把 scripting 落到它真正的层（stdlib workspace 成员）后：`z42c build --workspace`（`build stdlib`）直接编它、
入 flat dist，playground/wasm 与 z42.repl 从纯 stdlib dist 解析，特例步可整个删除。纯物理搬迁 + 构建重接，
不动任何 z42 源逻辑。

## What Changes

- **`git mv src/toolchain/scripting → src/libraries/z42.scripting`**（源/测试/README/toml 整体，包名不变）。
- `src/libraries/z42.workspace.toml` `default-members` 追加 `z42.scripting`（置末尾，依赖最多）。
- **删 `_buildScriptingLib` / `_buildReplStackToml` / `_scriptingLibsDir`**（`scripts/build/xtask_toolchain.z42`），
  新增 **`_buildReplLib`**：只建 `z42.repl`（tty 交互层，留 toolchain）→ libs，`Z42_LIBS=stdlib dist`
  （已含搬入的 scripting + 前端 z42c.core/syntax）。scripting 改由 `_buildStdlib` 编。
- `z42.interactive` 编译改用**纯 stdlib libs**（不再需 `.scripting-libs` 合并目录）——其编译期依赖闭包
  （scripting + repl + 前端）全在 libs。同步改 `xtask_toolchain.z42` 的 `_buildToolchain` 与
  `xtask_package_desktop.z42` 的 `_pkgStageToolchainComponents`。
- 文档：`organization.md`（scripting 入 stdlib 特例注 + 前端 z42c.core/syntax 补进工具链库行）、
  两个 README（libraries 增 scripting 行 / toolchain 删 scripting 行）、`repl.md`、`repl-input-completeness.md`、
  `bench/repl/BASELINE.md`、`repl_tests.rs` 注释路径同步。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/**` → `src/libraries/z42.scripting/**` | RENAME | git mv 整体搬迁（10 src + 9 tests 子目录 + README + toml） |
| `src/libraries/z42.scripting/z42.scripting.z42.toml` | MODIFY | 头注释：物理落 stdlib、不再走特例步 |
| `src/libraries/z42.scripting/README.md` | MODIFY | 「位置」段 + 构建命令改 stdlib workspace |
| `src/libraries/z42.workspace.toml` | MODIFY | `default-members` 追加 `z42.scripting` |
| `src/toolchain/repl/z42.repl.z42.toml` | MODIFY | 头注释：改由普通 toolchain build 步构建 |
| `scripts/build/xtask_toolchain.z42` | MODIFY | 删 `_buildScriptingLib`/`_buildReplStackToml`/`_scriptingLibsDir`，加 `_buildReplLib`；interactive 用纯 libs |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | 同上：`_buildReplLib` + interactive 纯 libs |
| `docs/design/stdlib/organization.md` | MODIFY | scripting 入 stdlib 特例注 + 前端补进工具链库行 |
| `src/libraries/README.md` | MODIFY | 表增 `z42.scripting` 行 |
| `src/toolchain/README.md` | MODIFY | 表删 `scripting/` 行（repl 行描述微调） |
| `docs/design/toolchain/repl.md` | MODIFY | scripting 已搬 libraries |
| `docs/book/src/toolchain/repl-input-completeness.md` | MODIFY | 代码指针路径同步（Completeness → libraries；ReplEditing → repl，修 PR1 遗留） |
| `bench/repl/BASELINE.md` | MODIFY | Script.z42 路径同步 |
| `src/runtime/src/corelib/repl_tests.rs` | MODIFY | 注释里 golden 测试路径同步 |

**只读引用（不改）**：
- `scripts/packages.toml` — stdlib-glob 自动发现 libs 里的 zpkg（scripting.zpkg 由 workspace build 入 libs → 自动入包），**无需改**。
- `.github/**` — stdlib 成员派生自 `default-members`（`_stdlibList`），CI 不硬编码成员表，**无需改**。
- `src/toolchain/workload/wasm/platform/src/lib.rs` — 按包名 mount zpkg（非源路径），**无需改**。
- `docs/book/src/compiler/project-build.md:74` — 历史坑注（fix-repl-sdk-layout 时状态），保留历史框定。

## Out of Scope

- **命名空间 `Z42.*` → `Z42c.*`**（User 定终态名）：后续可选，本次不改。
- **z42.repl 搬 stdlib**：真 tty + native 行编辑 builtin、平台绑定重、只被 z42.interactive 消费 →
  非跨平台 stdlib 料，留 toolchain（违轴 2「没必要就不入」）。等真有用户程序想复用 `Std.Repl` 再搬（门控 `Platform.Capabilities()`）。
- **scripting golden 测试接入自动回归**：现 `tests/<name>/driver.z42` 子目录格式非 stdlib file-unit 发现
  （需 flat `tests/*.z42` 或 `tests/<name>/source.z42`），随包搬后仍**不自动跑**（同现状；真覆盖在 Rust cargo test）。本次不改测试语义。

## 验证（本地不可验 → CI 权威）

本机 seed 墙 + z42vm 退出期挂起 → 本地不建全栈。GREEN 以 CI 为准，重点盯：
- `compile-toolchain`（现经 `build stdlib` 编 scripting；也编 z42.repl/interactive）
- **`compile-test-assets`**（`build test` 步——scripting 编译真门禁之一，下游 test-consume/test-vm-jit 依赖其 goldens）
- `verify-selfhost`（自举字节不动点：scripting 入 stdlib workspace 不应扰动 z42c 字节）
- `test-consume` / `test-host`（apphost 消费 + z42i 打包路径）

零格式 bump。
