# Tasks: SDK 库 —— 统一解析与部署

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`vm`（新运行期报错行为 + 清单契约）→ 完整流程。proposal / spec / design 经 2026-10-01 讨论定稿（用户：「确认，请你持续推进」）。

## 阶段 1：support（PR-1；仓内消费者不动）

### 1.1 可见性（D2 / D4）
- [ ] 先确认解析器能否按文件加入解析域；否则实现 `sdk-view/`（cache 目录下，硬链接，按允许集 + 源指纹重拼）
- [ ] exe / lib：声明的 SDK 库 + 闭包进视图；未声明 ⇒ E0494 + hint
- [ ] analyzer：整个编译器目录 → 全量视图（过滤 stdlib 名）
- [ ] hooks：`CompileRequest.HostExtension`（构造后赋值）；z42b `_loadProjectHooks` 置位；BuildSession 同 analyzer 处理
- [ ] driver 与 BuildSession 两条路径同一套逻辑

### 1.2 部署（D3 / D5）
- [ ] `_bundleExeDeps`：沿 zpkg 依赖表走 SDK 库传递闭包（含经 lib 间接引入）
- [ ] `deploy = "sdk"`：校验（只对 SDK 库合法）、不复制、侧车 `probing-paths` 追加 `${Z42_HOME}/programs/z42c`

### 1.3 VM 提示（D6）
- [ ] `probing.rs` 记录展开失败的 `${Z42_HOME}` 条目 + 依赖解析失败报错附提示 + Rust 单测

### 1.4 过渡与诊断（D8 / D9）
- [ ] `${compiler_libs}` 发 warning（新码），给出等价按名写法
- [ ] 新码登记 + `error-codes.md`
- [ ] CompilerFingerprint +1

### 1.5 测试（D10）
- [ ] `xtask_compiler_e2e_*` 开发树各格
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
- [ ] `scripts/xtask.z42.toml`：`"z42.project" = { version = "*", deploy = "sdk" }`（`z42.build` 同）
- [ ] 文档与示例去掉 `${compiler_libs}`
- [ ] 本地 GREEN；PR CI；合入后 main / nightly 实测

## 阶段 4：删宏（再一个 release 之后）
- [ ] 删 `_expandDepPathMacros` 的 `compiler_libs` 分支与 warning 码；文档
