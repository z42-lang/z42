# Tasks: SDK 库 —— 统一解析与部署

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`vm`（新运行期报错行为 + 清单契约）→ 完整流程。proposal / spec / design 经 2026-10-01 讨论定稿（用户：「确认，请你持续推进」）。

## 阶段 1：support（PR-1；仓内消费者不动）

### 1.1 可见性（D2 / D4）
- [x] 解析器按目录扫、但 `WsTier.Admits` 已能按包名过滤 ⇒ 用扫描 tier 的 `Hidden`，不拼视图目录（design D2 已改）
- [x] exe / lib：声明的 SDK 库 + 闭包放行；未声明 ⇒ E0494 点名 SDK 库 + 声明写法
- [x] analyzer：编译器目录里基础解析域中没有的全部包放行
- [x] hooks：改为 z42b 把编译器目录 zpkg 直接放进 `CompileRequest.Deps`（不新增跨包字段；#999）
- [x] driver 与 BuildSession 两条路径同一套逻辑（`SdkLibs.z42`）

### 1.2 部署（D3 / D5）
- [x] `_bundleExeDeps`：既有的 DEPS 传递闭包走查（#849）已覆盖；声明的 SDK 库解析自编译器目录 ⇒ 判私有、复制（e2e 实测）
- [x] `deploy = "sdk"`：校验（只对 SDK 库合法——须解析自编译器目录）、不复制、侧车 `probing-paths` 追加 `${Z42_HOME}/programs/z42c`

### 1.3 VM 提示（D6）
- [x] `probing.rs` 记录展开失败的 `${Z42_HOME}` 条目 + 依赖解析失败报错附提示 + Rust 单测（#998）

### 1.4 过渡与诊断（D8 / D9）
- [ ] `${compiler_libs}` 发 warning（新码），给出等价按名写法
- [ ] 新码登记 + `error-codes.md`
- [x] CompilerFingerprint 追加 `add-sdk-libs-visibility`

### 1.5 测试（D10）
- [x] `xtask_compiler_e2e_*` 开发树：可见性三格 + exe 复制闭包 + `deploy = "sdk"` 四格（不复制 / 侧车占位符 / 在「SDK」上可运行 / stdlib 误用报错）
- [ ] `xtask test dist` 发布态：hooks（今天红）、仅 runtime 运行复制闭包后的 exe、`deploy = "sdk"` 两种环境

### 1.6 文档
- [ ] reference：`z42-toml.md`（SDK 库、`deploy = "sdk"`、宏废弃）、`runtime-settings.md`（z42c 只合成占位符条目、SDK 提示）、
      `compile-time-extensions.md`（analyzer / hooks 自动可见）
- [ ] internals：`project-model.md` 解析域一节
- [ ] 本地 GREEN；PR CI

## 阶段 2：CI 与本地一致（PR-2；可与阶段 1 并行）
- [ ] `setup-z42-sdk` action（从 ci-bootstrap 搬出下载 + 回退链，装到 `.z42`，输出 `seed-id`）
- [ ] ci-bootstrap：种子 = `.z42`；[2/5] 用 `.z42` 编 xtask；xtask 跑在 `.z42/bin/z42vm` 上；核对两代路径（D7 ⚠️）
- [ ] compile-toolchain：artifact 带 `.seed-id`
- [ ] xtask-bootstrap-artifact：`setup-z42-sdk` + seed-id 比对（不一致则重编 xtask）
- [ ] CI 垫片对齐本地 apphost；删 Windows 拷贝启动；`Z42_PORTABLE_VM` 取舍以本地对照为准
- [ ] 文档：`ci.md`、`xtask.md`、`bootstrap-seed.md`
- [ ] 本地 GREEN（用 apphost 跑）；PR CI

## 阶段 3：use（阶段 1 进 nightly 之后，PR-3）
- [ ] **前置：开发树 stdlib flat 里的编译器包副本**（2026-10-01 实施阶段 1 时发现）：`build compiler` 的破环预建把
      z42.build / z42.project / z42.package / z42c.core / z42c.syntax 写进 `artifacts/build/libraries/dist/release`（= 开发树
      `Z42_LIBS`），`build stdlib` 末步又清掉。这段窗口里编 xtask：这些包被判「框架」⇒ 不复制 ⇒ flat 清掉后 xtask 运行期
      `MissingSymbolException: Z42.Project…`（#994 起的潜在问题；CI 不受影响——种子编 xtask 时 Z42_LIBS 是种子 libs）。
      阶段 3 改 `deploy = "sdk"` 后同一窗口里的校验也会误判「不是 SDK 库」。⇒ 破环预建改落独立目录（不进 Z42_LIBS），
      或解析时把 flat 里的编译器包也视为 SDK 库。先定方案再动 xtask。
- [ ] `scripts/xtask.z42.toml`：`"z42.project" = { version = "*", deploy = "sdk" }`（`z42.build` 同）
- [ ] 文档与示例去掉 `${compiler_libs}`
- [ ] 本地 GREEN；PR CI；合入后 main / nightly 实测

## 阶段 4：删宏（再一个 release 之后）
- [ ] 删 `_expandDepPathMacros` 的 `compiler_libs` 分支与 warning 码；文档
