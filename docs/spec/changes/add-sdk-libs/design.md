# Design: SDK 库

两条正交的轴：**可见**（编译期能不能引用）由工程 kind + 声明决定；**部署**（要不要复制进产物）由代码在哪运行决定。

| 消费方 | 可见 | 部署 |
|---|---|---|
| exe | stdlib 自动；SDK 库按名声明 | 默认复制 SDK 库 + 传递闭包；`deploy = "sdk"` 不复制、侧车补 `${Z42_HOME}/programs/z42c` |
| lib | 同上 | 不打包（交最终 exe） |
| analyzer / hooks | stdlib + SDK 库自动 | 不复制（宿主进程提供） |
| z42b / z42i / z42d | 同 exe | **不变**：`probing-paths = "../z42c"`（住在 SDK 里，相对路径与 SDK 版本天然一致） |
| xtask | 同 exe，阶段 3 起 `deploy = "sdk"` | 不复制；本地与 CI 都跑在 SDK 上 |

## D1：不新增定位机制

编译期 SDK 库目录沿用 `CompilerDomain.Dirs()`（`z42c.pipeline/src/BuildSession.z42`，#994 已把开发树档改指 `z42c.driver`
的 dist）。运行期沿用 VM 已有的 `${Z42_HOME}` 占位符展开（`probing.rs`：`Z42_HOME` → `Z42_PORTABLE_VM` 反推 → VM 自身位置）。

被拒的备选（初稿）：`Z42_SDK_LIBS` 环境变量 + SDK 清单 `sdk-libs` 字段 + VM 新增一档。用户裁定：SDK 库与 SDK 版本强绑定，
可随意指定的位置容易出问题，不引入；SDK 内相对位置 / `${Z42_HOME}` 已够用。

## D2：可见性 = 扫描 tier 的 `Hidden` 挡掉不放行的 SDK 库

现有解析器以**目录**为单位（`libsDirs`）。直接并入 `programs/z42c/` 有两个问题：①里面有整套 stdlib 副本；②目录里所有 SDK 库
都可见（exe / lib 要求「声明了才可见」）。今天的 `${compiler_libs}` 宏也有 ②（`_zpkgRefDirs` 并入的是被引用文件所在目录）。

做法（实施时定，取代初稿的「拼 `sdk-view/` 目录」）：`ZpkgPathSort._sortedZpkgsMulti` 本就按 `WsTier.Admits(di, name)` 逐包过滤，
`WsTier.Hidden` = 整个不可见的包名 —— 正是需要的能力，**不新增字段**。`z42c.pipeline/src/SdkLibs.z42`（driver 与 BuildSession 共用）：

- `Plan`：放行集 —— exe / lib = 按名声明（不写 path）的 SDK 库 + 沿 zpkg DEPS 的传递闭包；analyzer = 编译器目录里基础解析域中
  没有的全部包。放行集非空才把编译器目录追加到 `libsDirs` 末尾。
- `MergeTier`：复制一份 tier，`Hidden` 并上不放行的 SDK 库名；只用于扫描与 `DepIdentity`，调用方手里的 `tier`（workspace 语义）不动。
  放行集为空 ⇒ 原样返回 ⇒ z42c 自建 byte-identical。
- stdlib 副本：名字在基础解析域里已有 ⇒ 不进放行集；且排在 `libs/` 之后、按 basename 先到先得本就选不中。
- `ExtendDeclared`：放行集并进声明白名单（DepIndex 只索引「`z42.` 前缀 + 声明依赖」、E0497 按它放行）。
- `HiddenProviderOf`：E0494 时直接到编译器目录找提供该命名空间的、当前不可见的 SDK 库并点名（只在报错路径执行）。

hooks 不经 `Plan`：z42b 编 hooks 时把编译器目录的 zpkg 直接放进 `CompileRequest.Deps`（`builder_hooks.z42`，#999），已在基础解析域里。

## D3：exe 复制 = SDK 库传递闭包

`_bundleExeDeps` 的待拷名单今天是「直接依赖 ∪ path 依赖闭包」。新增：从「待拷名单 ∪ path 闭包里每个 lib 的 zpkg」出发，
沿 `ZpkgReader.ReadDependencies` 走，命中 SDK 库（不在 `libs/`）的包加入待拷名单，直到不动点。声明了 `deploy = "sdk"` 的依赖
及仅因它进入闭包的包不拷。z42b `publish` 用 z42c 产物（`_bundleExeDeps` 已 colocate），不另写。

## D4：analyzer / hooks 走「宿主扩展」通道

- analyzer：`kind == "analyzer"` 时把原来的「并入整个编译器目录」换成 D2 的全量视图（去掉 stdlib 副本）。
- hooks：z42b `_loadProjectHooks` 编译时 `CompileRequest` 构造后赋值 `HostExtension = true`（**不进 ctor 签名**——ctor 是种子
  ABI，同 `DepEntry.Deploy` 惯例）；BuildSession 见到它与 analyzer 同等处理。hooks 现有的 stdlib 依赖面（`Z42_LIBS` 全部 zpkg）保留。
- 两者都是 lib 形态，天然不打包。hooks 的 `z42.build` 必须绑定 z42b 已加载的那份：ModuleLoader 按名加载时先查已加载模块——
  **实施时用 spec 的「类型同一性」场景在发布态实测**；不成立则在 `_loadProjectHooks` 里显式把宿主已加载模块注册为解析源。

## D5：`deploy = "sdk"`

- 取值校验在 `_validateDeployDecls`；「只对 SDK 库合法」在解析后判（解析源是 SDK 视图 ⇒ 合法）。
- z42c 生成侧车时，若有任一 `deploy = "sdk"` 依赖，在 `[runtime] probing-paths` 末尾追加 `${Z42_HOME}/programs/z42c`
  （与清单 `[profile.*.runtime]` 的条目合并、去重）。
  - 这改变了一条既有原则「z42c 不合成 probing-paths，侧车里的值是清单逐字拷贝」（`runtime-settings.md`）。该原则的目的是
    「构建输出里不出现具体路径」——追加的只是占位符，目的不受影响；文档改述为「z42c 只合成占位符形式的条目」。
- 与 `shared` 的关系：`shared` = 不拷，运行期由作者写的 `probing-paths` 解析；`sdk` = 不拷，由 z42c 补上指回 SDK 的那一条。

## D6：VM 提示

`probing.rs` 展开时记下「含 `${Z42_HOME}` 且展开后不存在」的条目（原样模式串）。依赖解析失败的报错点（`MissingSymbolException` /
找不到依赖 zpkg）若该记录非空，附：

```
probing 路径 ${Z42_HOME}/programs/z42c 无法解析 —— 是否没有安装 z42 SDK？（安装 SDK，或设置 Z42_HOME 指向 SDK 根目录）
```

记录为空 ⇒ 报错照旧。对所有用 `${Z42_HOME}` 的程序生效，不限 `deploy = "sdk"`。

## D7：CI 与本地一致——xtask 跑在 SDK 上

本地：`install-z42.sh` 把 nightly SDK 装到 `.z42/` → `.z42/z42 publish scripts/xtask.z42.toml` 产出 `./xtask` apphost →
apphost 从自身目录向上找到 `.z42`，用其 `bin/z42vm` + `libs/` 跑 xtask（不设 `Z42_HOME`）。xtask 构建 / 测试当前源码时的子进程
用构建树工具（xtask 自己定位）。⇒ **编 xtask 与跑 xtask 是同一个 SDK**，版本严格一致。

CI 改为同一形态：

- 新 composite action **`setup-z42-sdk`**：把 ci-bootstrap 里「下载 nightly SDK（10 次重试）→ 回退最近成功 CI 运行的
  `release-host-<rid>` 归档」整段搬出来，安装到 `$GITHUB_WORKSPACE/.z42`（与本地同路径），输出 `seed-id`（nightly 的
  target commit，回退时为该 run 的 head sha）。ci-bootstrap 与 xtask-bootstrap-artifact 都用它 ⇒ 回退链只有一份。
- **ci-bootstrap**：种子 = `.z42`（不再用完即删）；[2/5] 用 `.z42` 的 z42c 编 xtask；之后的 xtask 调用都跑在 `.z42/bin/z42vm` 上。
- **compile-toolchain**：toolchain artifact 里带上 `artifacts/xtask/.seed-id`。
- **xtask-bootstrap-artifact**：先 `setup-z42-sdk`（本 job 宿主 RID）；`seed-id` 与 artifact 里的一致 ⇒ 直接用 artifact 的
  xtask.zpkg；不一致（中途 nightly 被重发）⇒ 用本 job 的 `.z42` 重编 xtask（~1 min，罕见）。
- **CI 垫片 `.github/ci/xtask`**：等价于本地 apphost——`.z42/bin/z42vm` + `Z42_LIBS=.z42/libs` 跑 xtask.zpkg。Windows 上「从拷贝
  启动以免 cargo 重链覆盖正在运行的 z42vm.exe」的特殊处理随之删除（SDK 的 VM 不会被 cargo 重链）。是否保留
  `Z42_PORTABLE_VM=<构建树 vm>`：以本地 `./xtask` 的行为为准——实施时在本地用 apphost 跑一遍 GREEN 与 CI 各步骤的命令对照。
- **两代自举（格式 bump）**：今天 [2/5] 在两代路径下改用 gen2 编 xtask，原因是 xtask 跑在新 VM 上、新 VM 读不了旧格式。xtask 跑在
  种子 VM 上之后，[2/5] 一律用种子编即可；两代逻辑只服务于「构建当前 z42c / stdlib」。⚠️ 实施时逐条核对 1.5 段各步的 VM / libs
  选择，确认无遗漏后再删 [2/5] 的 gen2 分支。

## D8：诊断码

新登记（`DiagnosticCodes.z42` + `docs/reference/src/appendix/error-codes.md`，过 `xtask test diagcodes`）：

- 未声明却引用了 SDK 库的命名空间：沿用 E0494，补 hint（无新码）。
- `deploy = "sdk"` 用在非 SDK 库：新 Error 码。
- `${compiler_libs}` 宏：新 Warning 码（过渡期；删宏时退役）。

## D9：自举与缓存

- 分阶段（proposal）：种子 z42c 不认识 `deploy = "sdk"` ⇒ 阶段 3 必须等阶段 1 进 nightly。D7（阶段 2）不依赖新语义，可先行。
- z42c 自建（kind = exe，不声明 SDK 库）解析域不变 ⇒ 自举 byte-identical。
- CompilerFingerprint +1：解析域进入语义分析的输入。

## D10：测试

- Rust：`probing.rs` 单测——`${Z42_HOME}` 条目展开失败被记录；无此类条目时不记录；报错附提示 / 不附提示两格。
- z42：`xtask_compiler_e2e_*`（开发树）——声明可见 / 未声明不可见 + hint；stdlib 副本不进产物；传递闭包；经 lib 间接；
  `deploy = "sdk"`（产物无副本、侧车含占位符条目）；非 SDK 库用 `sdk` 报错；宏 warning。
- 发布态（`xtask test dist`，package-host ×4）：**hooks 在装好的 SDK 上编译 + 加载 + 类型同一性**（今天红的那条）；复制闭包后的
  exe 在仅 runtime 包环境可运行；`deploy = "sdk"` 在 SDK 上可运行、仅 runtime 时报错含 SDK 提示。
- CI 一致性（阶段 2）：CI 全绿；各 job 日志确认 xtask 由 `.z42/bin/z42vm` 运行；手工制造 seed-id 不一致，确认走重编分支。
