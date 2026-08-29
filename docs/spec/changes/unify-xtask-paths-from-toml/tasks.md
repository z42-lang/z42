# Tasks: xtask 构建路径从 workspace.toml 单源获取

> 状态：🟡 进行中 | 创建：2026-08-29 | 类型：refactor（最小化模式）

**变更说明：** 把 xtask 脚本里散落的构建树布局硬编码（per-member dist / flat dist / driver zpkg / build root / runtime out）收敛到一个布局模块 `scripts/common/xtask_layout.z42`；其中 per-member dist 改为**读 workspace.toml 的 `[workspace.build].output_dir` 模板**（经 `z42.project` 的 `ManifestLoader.LoadWorkspace` + `PathTemplate.Expand`）——与 z42c 的 `WorkspaceBuild.PlanLayout` 共享同一份布局真相。

**原因：** 布局此前同时编码在两处——`src/{libraries,compiler}/z42.workspace.toml` 的模板（z42c 消费）+ xtask 里 ~40 处 `"artifacts/build/..."` 字面拼接。改布局要两头动、且靠"碰巧一致"。todo-list #8。

**文档影响：** `scripts/README.md`（路径来源说明）；无外部行为变化（同样的解析路径，仅来源改为单源）。

## Scope（build 布局族；不含一次性 scratch/test-tooling 临时目录）
- `scripts/common/xtask_layout.z42` — NEW，布局模块
- `scripts/common/xtask_common.z42` — MODIFY，`_libsDir` 委托到 `_libsFlatDist`
- `scripts/build/xtask_stdlib.z42` — MODIFY
- `scripts/build/xtask_compiler.z42` — MODIFY
- `scripts/build/xtask_compiler_e2e.z42` — MODIFY
- `scripts/build/xtask_bootstrap_check.z42` — MODIFY
- `scripts/README.md` — MODIFY，路径来源说明

**明确不改**（尽量简化：每处 1 次、无 toml 归属、非布局重复）：`artifacts/.scratch/*`、`artifacts/tools/*`、`artifacts/{tmp,test-reports,profile,publish}`、各 `*-test` 测试暂存目录。

## 进度
- [x] 1. 布局模块 xtask_layout.z42（helpers + `_cleanPath` + 读 toml 的 per-member）
- [x] 2. 核心 helper 本地验证（GOT==WANT，独立 ztest 程序验证 ✓ libraries/compiler·release/debug）
- [x] 3. 替换 xtask_common / xtask_stdlib / xtask_compiler / *_e2e / bootstrap_check / toolchain / install_vscode 的布局字面量
- [x] 4. 编译验证：种子 z42c 编 xtask 项目无错（两次，含新增两处 driver）
- [x] 5. scripts/README.md 同步路径来源
- [ ] 6. GREEN（PR CI 完整 gate；本机因 stale-seed 无法跑完整自举，见备注）

## 备注
- 核心机制（读 toml + `_cleanPath` 折叠 `../../`）已用独立 `ztest` 程序本地跑通：GOT==WANT（libraries/compiler、release/debug 均对）。
- 种子 z42c 编 xtask 整个项目**零错误**（两次编译），确认所有 helper 调用 + `_verifyMemberZpkgs` 签名变更类型正确。
- z42 stdlib 无 `Path.Normalize` → 模块自带 `_cleanPath` 折叠 `..`，保持 canonical 形态（xtask 有前缀 strip 等文本操作，`..` 路径会破坏）。
- **本机完整自举被 stale-seed 挡住（与本改动无关）**：本机所有 stdlib 种子（主树 .z42/artifacts）都引用 builtin `__int32_equals`，而它已被 origin/main 的 `shrink-primitive-native-interop`（2026-08-27）移除 → fresh z42vm 加载种子 stdlib panic。无 post-shrink 本地种子；建之需两代自举/下载 nightly（CI ci-bootstrap 自动做）。路径改动只改 zpkg 写读**位置**、不碰 builtin/内容，逻辑上不可能引入此 panic。完整字节不动点 GREEN 交 PR CI。
