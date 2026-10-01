# Tasks: SDK 库 —— 统一解析与部署

**状态：🟡 待确认（proposal / spec / design 待 User 审阅）| 开始：2026-10-01**

类型：`vm`（新运行期解析行为 + 清单契约）→ 完整流程。

## 阶段 1：support（一个 PR；仓内消费者不动）

### 1.1 定位（D1）
- [ ] `z42.project`：`SdkLibs.Dirs()`（`Z42_SDK_LIBS` → SDK 根 `manifest.toml` 的 `sdk-libs`（缺省 `programs/z42c`）→ 开发树）
- [ ] `z42c.pipeline` 的 `CompilerDomain.Dirs()` 改为委托 `SdkLibs.Dirs()`，调用点不变
- [ ] 打包：SDK 包 `manifest.toml` 写 `[contents] sdk-libs`（`scripts/packages.toml` 驱动）
- [ ] VM `probing.rs`：`sdk_lib_dirs()`（复用 `z42_home_roots()` + 读清单字段 + `Z42_SDK_LIBS`）+ 单测

### 1.2 可见性（D2 / D4）
- [ ] 先确认解析器能否按文件加入解析域；否则实现 `sdk-view/`（cache 目录下，硬链接，按声明集 + 内容指纹重拼）
- [ ] exe / lib：声明的 SDK 库 + 闭包进视图；未声明 ⇒ E0494 + hint
- [ ] analyzer：整个编译器目录 → 全量视图（过滤 stdlib 名）
- [ ] hooks：`CompileRequest.HostExtension`（构造后赋值）；z42b `_loadProjectHooks` 置位；BuildSession 同 analyzer 处理
- [ ] driver 与 BuildSession 两条路径同一套逻辑

### 1.3 部署（D3 / D5）
- [ ] `_bundleExeDeps`：沿 zpkg 依赖表走 SDK 库传递闭包（含经 lib 间接引入）
- [ ] `deploy = "sdk"`：校验（只对 SDK 库合法）、不复制、侧车 `[runtime] sdk-libs = true`；清单手写该键报错
- [ ] VM：`sdk-libs = true` ⇒ 追加 SDK 库目录；解析失败时的指定报错文本

### 1.4 过渡与诊断（D6）
- [ ] `${compiler_libs}` 发 warning（新码），给出等价按名写法
- [ ] 新码登记：`deploy = "sdk"` 误用、手写 `sdk-libs`、宏 warning；`error-codes.md`
- [ ] CompilerFingerprint +1（D7）

### 1.5 测试（D9）
- [ ] Rust 单测
- [ ] `xtask_compiler_e2e_*` 开发树各格
- [ ] `xtask test dist` 发布态：hooks（今天红）、仅 runtime 运行复制闭包后的 exe、`deploy = "sdk"` 两种环境

### 1.6 文档
- [ ] reference：`z42-toml.md`（SDK 库、`deploy = "sdk"`、宏废弃）、`runtime-settings.md`（`sdk-libs`、`Z42_SDK_LIBS`）、
      `compile-time-extensions.md`（analyzer / hooks 自动可见）、SDK 清单字段
- [ ] internals：`project-model.md` 解析域一节、`vm-architecture.md` 搜索序
- [ ] 本地 GREEN；PR CI

## 阶段 2：use（阶段 1 进 nightly 之后，另一个 PR）
- [ ] `scripts/xtask.z42.toml`：`"z42.project" = { version = "*", deploy = "sdk" }`（`z42.build` 同）
- [ ] z42b / z42i / z42d 清单：按名 + `deploy = "sdk"`，删 `probing-paths = "../z42c"`
- [ ] `.github/ci/xtask`、ci-bootstrap（[2/5] 之后与 xtask-only 分支）、本地启动说明：`Z42_SDK_LIBS`（D8）
- [ ] 文档与示例去掉 `${compiler_libs}`
- [ ] 本地 GREEN；PR CI；合入后 main / nightly 实测

## 阶段 3：删宏（再一个 release 之后）
- [ ] 删 `_expandDepPathMacros` 的 `compiler_libs` 分支与 warning 码；文档
