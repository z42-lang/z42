# z42.project

## 职责
项目清单 `z42.toml` 的**类型化模型**（全 z42）。作为 z42c（编译器）与 z42.build
（发布管线）**共同依赖的单一真相**：一处定义 schema，两处复用，避免模型重复与漂移。

工程配置是**确定的**——字段固定、不开放任意自定义键（含 `[platform.*]` 也用 typed
固定字段，不用开放 map）。模型 + `ManifestLoader`（TOML → 组合式模型）齐备，用 `Std.Toml`
解析，fs-free 入口（`ParseText` / `ParseWorkspaceText`）可供 REPL / playground 复用。

> ⚠️ **Parked / 接口先行（2026-06-18；loader 补齐 2026-06-29）**：受限自举子集写法
> （sealed class + 构造函数、`bool HasX` 替 nullable、`array + count` 替泛型；无 record /
> 无泛型 / 无 nullable），与 `src/compiler/z42c.project` 同子集，类型名对齐（`DepEntry` /
> `WorkspaceManifest`）便于日后 z42c 直接引用本库（届时删 z42c 自带的 ProjectModel）。
> **暂不接入编译**（无 `z42.project.z42.toml`，不登记 workspace/xtask/CI，零编译参与）；
> 验证留待「接入」时（建清单 + 登记 + GREEN）。schema 以 `docs/design/compiler/project.md`
> 为准。**当前 `Z42.Project` 与 z42c.project 暂并存两份**（User 决策：z42.project 按最终方案
> 写，z42c 后续引用）。

## 核心文件
| 文件 | 段 | 职责 |
|------|----|------|
| `src/ManifestLoader.z42` | — | TOML → 模型 加载器：`Load`/`ParseText`（单项目）、`LoadWorkspace`/`ParseWorkspaceText`（workspace）；解析全段含 `[profile.*]`/`[[exe]]`/`[platform.*]`/`[optimize]`/`[analyzers]`/`[lints]`/`[native.*]`/`[tests]`·`[benches]`·`[examples]`/`[[test]]`·`[[bench]]`·`[[example]]` |
| `src/NativeSpec.z42` | `[native.<name>]` | 本包携带的私有 native 库声明（逻辑名 `Name` + 可选预编译基目录 `Dir`；文件名平台派生 `<prefix><name><suffix>`）。消费方 z42b publish 沿闭包：有 `[build] hooks` 跑 `ProvideNative`、否则从 `Dir` 按 rid 复制预编译文件 → 平铺进 payload（add-native-dep-config / add-precompiled-native） |
| `src/SourceDiscovery.z42` | — | `[sources].include` glob → 绝对路径列表（递归/单层，排除 dist/.cache，去重+Ordinal 排序）|
| `src/PathTemplate.z42` | — | 路径模板展开（`${project_name}`/`${profile}`/`${output_dir}` 等）+ `TemplateContext` |
| `src/ProjectManifest.z42` | 根 | 聚合各段的完整清单（单项目） |
| `src/ProjectInfo.z42` | `[project]` | name / version / kind / entry / pack |
| `src/Sources.z42` | `[sources]` | include / exclude glob（array + count） |
| `src/BuildConfig.z42` | `[build]` | output_dir / cache_dir / dist_dir / incremental |
| `src/Profile.z42` | `[profile.*]` | pack / strip / mode / optimize / debug |
| `src/ProjectManifest.z42`（`OptimizeNames`/`Values`/`Count`） | `[optimize]` | 逐 pass 具名开关（`inline=true`/`const-fold=false`…）中性 name/value 对；消费方按名映射编译器 `Opt` 位（add-compiler-inlining；消费受两-nightly 纪律） |
| `src/ProjectManifest.z42`（`Analyzers`/`AnalyzerCount`） | `[analyzers]` | 编译期 handler zpkg 引用（DepEntry name/version）——「加载进编译器、编译期运行、不链入目标程序」（attribute-handler-registry D9）；消费方（z42c）按名在 LibsDirs 解析到 `<name>.zpkg` 加载其 `: Analyzer` 类型 |
| `src/ProjectManifest.z42`（`LintNames`/`LintSeverities`/`LintCount`/`LintWarningsAsErrors`） | `[lints]` | 诊断规则 severity 覆盖（`Z9002="warning"`/`"pkg.*"="none"`）中性 name/severity 串对 + `warnings-as-errors` 布尔（attribute-handler-registry PR3b）；z42.project 不解释规则/级别语义，消费方（z42c `LintConfig`）做 `EnabledByDefault`+覆盖+通配+WAE 决策 |
| `src/DepEntry.z42` | `[dependencies]`·`[analyzers]` | 单项依赖（name / version / **path**）。`path` 非空 = 本地路径依赖（`{ path="../foo" }`，源在相对 manifest 目录处）；为 "" = 名字依赖走 Z42_LIBS。解析 support（add-path-dependencies PR-1）；z42c 消费闭包构建/打包受两-nightly 纪律（PR-2） |
| `src/ExeTarget.z42` | `[[exe]]` | 多 exe 目标 |
| `src/TargetSection.z42` | `[tests]`·`[benches]`·`[examples]` | dev 目标段：约定发现 glob（include/exclude/auto）+ dev-deps 隔离 |
| `src/RunTarget.z42` | `[[test]]`·`[[bench]]`·`[[example]]` | dev 运行目标（三类共用）：name / harness / entry / sources / deps / test（example 纳入 xtask test 执行）|
| `src/PlatformSet.z42` | `[platform]` | 四平台 typed 配置集合（HasX 标志） |
| `src/iOSConfig.z42` | `[platform.ios]` | bundle_id / 能力 / team_id / device_families |
| `src/AndroidConfig.z42` | `[platform.android]` | app_id / version_code / sdk / permissions |
| `src/DesktopConfig.z42` | `[platform.desktop]` | publish_dir / icon / bundle_id |
| `src/WasmConfig.z42` | `[platform.wasm]` | title |
| `src/WorkspaceManifest.z42` | `[workspace]` | monorepo 成员（单独解析） |

## 入口点
- `ManifestLoader.Load(path)` / `ManifestLoader.ParseText(text)` —— 单项目清单加载
- `ManifestLoader.LoadWorkspace(path)` / `ParseWorkspaceText(text)` —— workspace 清单加载
- `SourceDiscovery.Discover(projectDir, includes, count)` —— 源文件 glob 发现
- `PathTemplate.Expand(template, ctx)` —— 路径模板展开
- `ProjectManifest` —— 单项目清单根模型（含 `PlatformSet Platform`）
- `WorkspaceManifest` —— workspace 根清单模型

## 依赖关系
- `Std.Toml`（TOML 解析）、`Std.IO`（文件读取，仅 `Load*` 路径）；
  下游：`z42.build` / `z42b`、（接入后）z42c
