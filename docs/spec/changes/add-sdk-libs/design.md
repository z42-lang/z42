# Design: SDK 库

两条正交的轴：**可见**（编译期能不能引用）由工程 kind + 声明决定；**部署**（要不要复制进产物）由代码在哪运行决定。

| 消费方 | 可见 | 部署 |
|---|---|---|
| exe | stdlib 自动；SDK 库按名声明 | 默认复制 SDK 库 + 传递闭包；`deploy = "sdk"` 不复制、运行期从所在 SDK 解析 |
| lib | 同上 | 不打包（交最终 exe） |
| analyzer / hooks | stdlib + SDK 库自动 | 不复制（宿主进程提供） |
| xtask、z42b / z42i / z42d | 同 exe，`deploy = "sdk"` | 不复制 |

## D1：SDK 库目录的唯一出处 = SDK 自己的清单

`<sdk>/manifest.toml` `[contents]` 新增 `sdk-libs = ["programs/z42c"]`（打包时由 `scripts/packages.toml` 驱动写入）。
缺省 `["programs/z42c"]`，旧 SDK 不受影响。

定位（spec「定位序」）有两份实现，无法合一：

- z42 侧：`z42.project` 新增 `SdkLibs.Dirs()`，**取代** `z42c.pipeline` 的 `CompilerDomain.Dirs()`（driver、BuildSession、
  z42b 共用）。z42.project 是零编译器依赖的包，z42b 够得着。
- Rust 侧：`src/runtime/src/probing.rs` 新增 `sdk_lib_dirs()`，复用已有的 `z42_home_roots()`（`Z42_HOME` →
  `Z42_PORTABLE_VM` → `current_exe`）再读清单字段。

**VM 不认开发树布局**（第 ③ 档只在 z42 侧）：VM 是随 runtime 包发给最终用户的，不该知道 `artifacts/build/…`。
开发树 / CI 里运行 `deploy = "sdk"` 的程序（xtask）由启动方设 `Z42_SDK_LIBS`。两份实现的一致性由 e2e 守（D9）。

被拒的备选：

- 在用户清单里保留路径宏（`${compiler_libs}`）—— 用户明确不要：SDK 内部一调整就失效。
- 全局开关（`sdk-libs = true` 让所有 SDK 库免声明）—— 丢隔离，清单里也看不出依赖了不稳定 API。

## D2：可见性 = 一个按需拼出的「SDK 视图」目录

现有解析器以**目录**为单位（`libsDirs`）。直接把 `programs/z42c/` 并进去有两个问题：①它里面有整套 stdlib 副本；
②目录里所有 SDK 库对本工程都可见（exe / lib 要求「声明了才可见」）。今天的 `${compiler_libs}` 宏也有 ② 的问题
（`_zpkgRefDirs` 并入的是**被引用文件所在的目录**）。

做法：每次构建在本工程 cache 目录下拼一个 `sdk-view/`，里面只放**允许本工程看见的 SDK 库**（硬链接，失败回落复制），
把它作为 `libsDirs` 的最后一项：

- exe / lib：声明的 SDK 库 + 它们在 SDK 库内的传递闭包（编译期也要看得见闭包里的类型，否则跨包签名解析不全）；
- analyzer / hooks：SDK 库目录里全部「`libs/` 中不存在的包名」。

stdlib 过滤在拼视图时一次完成：包名在 `libs/` 里存在 ⇒ 不进视图。

视图是派生物，按「声明集 + SDK 库目录内容指纹」决定是否重拼；不进 dist。

⚠️ 实施时先确认：解析器是否本就支持按文件（而非目录）加入解析域——若支持，用文件列表代替视图目录，更省事。

## D3：exe 复制 = SDK 库传递闭包

`_bundleExeDeps` 的待拷名单今天是「直接依赖 ∪ path 依赖闭包」。新增一步：从「待拷名单 ∪ path 闭包里每个 lib 的 zpkg」出发，
沿 `ZpkgReader.ReadDependencies` 走，凡命中 SDK 库目录（且不在 `libs/`）的包加入待拷名单，直到不动点。
写了 `deploy = "sdk"` 的依赖及其闭包**不拷**（闭包里若有别的依赖声明为默认，仍按默认拷——以声明为准，冲突时 `sdk` 优先，
保证运行期只有一份）。

z42b `publish` 的打包走的是 z42c 产物（`_bundleExeDeps` 已 colocate），不另写一份。

## D4：analyzer / hooks 走「宿主扩展」通道

- analyzer：沿用 `kind == "analyzer"` 判定，把原来的「并入整个编译器目录」换成 D2 的全量视图。
- hooks：z42b `_loadProjectHooks` 编译时，`CompileRequest` 构造后赋值 `HostExtension = true`（**不进 ctor 签名**——ctor
  是种子 ABI，同 `DepEntry.Deploy` 的惯例）。BuildSession 见到它与 analyzer 同等处理。今天 hooks 的依赖面
  「`Z42_LIBS` 全部 zpkg」保留（stdlib），SDK 库改由视图提供。
- 两者都是 lib 形态，天然不打包。运行期 hooks 的 `z42.build` 必须绑定 z42b 已加载的那份：ModuleLoader 按名加载时先查已加载
  模块——**实施时用 spec 的「类型同一性」场景实测确认**，不成立则在 `_loadProjectHooks` 里显式把宿主已加载模块注册为解析源。

## D5：`deploy = "sdk"`

- 取值校验在 `_validateDeployDecls`；「只对 SDK 库合法」需在解析后判（解析源是 SDK 视图 ⇒ 合法）。
- 侧车：z42c 生成 runtimeconfig 时，若有任一 `deploy = "sdk"` 依赖，在 `[runtime]` 写 `sdk-libs = true`（与清单
  `[profile.*.runtime]` 合并；清单里不允许手写该键，避免两个来源）。
- VM：读到 `sdk-libs = true` ⇒ `search_dirs = [entry_dir, probing…, libs, sdk_lib_dirs…]`。模块解析失败且 `sdk-libs = true`
  ⇒ 报错文本附「需在 z42 SDK 上运行（或设 Z42_SDK_LIBS）」。
- 与 `shared` 的关系：`shared` = 「不拷，运行期由作者写的 `probing-paths` 解析」；`sdk` = 「不拷，运行期由 VM 按 SDK 清单解析」。
  `sdk` 是 `shared` 的一个**不需要作者知道路径**的特例。

## D6：诊断码

新登记（`DiagnosticCodes.z42` + `docs/reference/src/appendix/error-codes.md`，过 `xtask test diagcodes`）：

- 未声明却引用了 SDK 库的命名空间：沿用 E0494，补 hint（无新码）。
- `deploy = "sdk"` 用在非 SDK 库：新 Error 码。
- `${compiler_libs}` 宏：新 Warning 码（过渡期；删宏时随之退役）。
- 清单手写 `[runtime] sdk-libs`：新 Error 码。

## D7：自举与缓存

- **分阶段**（proposal）：种子 z42c 不认识 `deploy = "sdk"` ⇒ 阶段 2 必须等阶段 1 进 nightly；VM 每次 CI 都从源码 cargo 编，
  阶段 1 起即可用。
- z42c 自建（kind = exe，不声明 SDK 库）解析域不变 ⇒ 自举 byte-identical。
- **CompilerFingerprint**：解析域进入语义分析的输入（同样的源，可见包集合不同 ⇒ 结果可能不同），保守起见 +1。
- xtask 不复制后的**版本一致性**：编 xtask 的工具链 T1 与运行时的 SDK 库 T2 可能不同（CI：种子编、构建树跑）。与 xtask
  对 stdlib 的处境完全相同，由同一条纪律兜底——SDK 库的 API **先加后用、删除两阶段**（`bootstrap-seed.md`「stdlib API 面」
  本已把 `Z42.Project` / `Z42.Build` 列入，见 #994）。

## D8：xtask 在各运行场景的 SDK 库来源（阶段 2）

| 场景 | 来源 |
|---|---|
| ci-bootstrap [3/5][4/5] | `Z42_HOME` = 种子 SDK（= 编出 xtask 的那份） |
| CI 垫片（`.github/ci/xtask`）后续步骤 | `Z42_SDK_LIBS` = `artifacts/build/compiler/z42c.driver/release/dist` |
| docs-check（xtask-only） | 经 `GITHUB_ENV` 导出 `Z42_SDK_LIBS` = 种子目录 |
| test-consume | 下载的 current-sdk 的 `bin/z42vm` ⇒ VM 自推 SDK 根 |
| 本地 `./xtask`（`.z42` SDK publish 出的 apphost） | apphost 用 `.z42` 的 VM ⇒ 自推 |
| 本地直接 `z42vm artifacts/xtask/xtask.zpkg` | 启动方设 `Z42_SDK_LIBS`（同 CI 垫片） |

## D9：测试

- Rust：`probing.rs` 单测——`Z42_SDK_LIBS` 优先、清单字段读取、缺省 `programs/z42c`、`sdk-libs = false` 时不追加。
- z42：`xtask_compiler_e2e_*`（开发树）——声明可见 / 未声明不可见 + hint；stdlib 副本不进产物；传递闭包；经 lib 间接；
  `deploy = "sdk"` 产物无副本、侧车有 `sdk-libs = true`；非 SDK 库用 `sdk` 报错；宏 warning。
- 发布态（`xtask test dist`，package-host ×4）：**hooks 在装好的 SDK 上编译 + 加载 + 类型同一性**（今天红的那条）；
  exe 复制闭包后在**仅 runtime 包**环境可运行；`deploy = "sdk"` 在 SDK 上可运行、仅 runtime 时报指定错误。
