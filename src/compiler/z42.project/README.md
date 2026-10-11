# z42.project

## 职责
项目清单 `z42.toml` 的**类型化模型**（全 z42）。作为 z42c（编译器）与 z42.build（发布管线）共同依赖的单一真相：一处定义 schema，多处复用，避免模型重复与漂移。

工程配置是**确定的**——字段固定、不开放任意自定义键（含 `[platform.*]` 也用 typed 固定字段，不用开放 map）。
合法键表在 `ManifestKeys`；写了不认识的键（以及 `[lints]` 里不合法的值）由 loader 收集进 `UnknownKeys`，消费方报错。
受限自举子集写法：sealed class + 构造函数、`bool HasX` 替 nullable、`array + count` 替泛型。
schema 以 [z42-toml.md](../../../docs/reference/src/toolchain/z42-toml.md) 为准。

## 功能索引
命名空间 `Z42.Project`。

| 能力 | 入口 |
|------|------|
| 加载单项目清单 | `ManifestLoader.Load(path)` / `ParseText(text)`（fs-free，REPL / playground 可复用）|
| 加载 workspace 清单 | `ManifestLoader.LoadWorkspace(path)` / `ParseWorkspaceText(text)` |
| 清单键审计 | `ManifestKeys.AuditProject(root)` / `AuditWorkspace(root)` → `KeyAudit`（`Unknown()` / `Deprecated()`）|
| 源文件 glob 发现 | `SourceDiscovery.Discover(projectDir, includes, count)` |
| 路径模板展开 | `PathTemplate.Expand(template, ctx)` |
| 清单定位 | `ManifestLocator.FindUp` / `FindIn` |
| 产物目录解析 | `BuildLayout.Resolve(manifest, projectDir, isRelease)` |

## 如何测试验证
```bash
./xtask test stdlib z42.project    # tests/*.z42 全部单元
```
全部 `[Test]` 通过即成立。

## 关联文档
- schema：[z42-toml.md](../../../docs/reference/src/toolchain/z42-toml.md)
- 项目模型机制：[project-model.md](../../../docs/internals/src/compiler/project-model.md)

## 核心文件
### 加载与定位
| 文件 | 职责 |
|------|------|
| `src/ManifestLoader.z42` | TOML → 模型 加载器；解析全段含 `[profile.*]`/`[[exe]]`/`[platform.*]`/`[optimize]`/`[analyzers]`/`[lints]`/`[native.*]`/`[tests]`·`[benches]`·`[examples]`/`[[test]]`·`[[bench]]`·`[[example]]` |
| `src/ManifestKeys.z42` | 各段合法键表（清单契约的唯一真相源）+ `KeyAudit` 审计；loader 构造后调用，结果挂到 `UnknownKeys` |
| `src/ManifestLocator.z42` | 清单定位：`FindUp`（从目录向上找 `z42.toml` → 唯一 `*.z42.toml` → `z42.workspace.toml`）/ `FindIn`（只看一层）/ `ErrorText`；launcher / z42b / z42c 共用 |
| `src/SourceDiscovery.z42` | `[sources].include` glob → 绝对路径列表（递归/单层，排除 dist/.cache，去重 + Ordinal 排序；`exclude` 走 `PathGlob`）|
| `src/PathGlob.z42` | 按路径段的 glob 匹配（`**` 跨段、`*`/`?` 段内），`[sources].exclude` 与 `[workspace] members/exclude` 共用 |
| `src/PathTemplate.z42` | 路径模板展开（`${project_name}`/`${profile}`/`${output_dir}` 等）+ `TemplateContext` |

### 布局与依赖规划
| 文件 | 职责 |
|------|------|
| `src/BuildLayout.z42` | 产物目录级联：`Resolve` → output / cache / generated / dist；z42c 写、launcher run 找、z42b clean 删共用。`Display(path)` 把目录呈现给终端（相对当前目录，带 `./`），进度行统一走它 |
| `src/WorkspaceLayout.z42` | workspace 成员判定与成员产物布局的唯一来源；`BuildLayout.Resolve` 与 workspace 构建都经由它 |
| `src/PathDepPlan.z42` | 路径依赖 `dep = { path = "../foo" }` 的传递闭包规划（拓扑序，叶子在前）；`z42c build` 与 `z42b build` 共用一份实现 |

### 清单模型（按段）
| 文件 | 段 | 职责 |
|------|----|------|
| `src/ProjectManifest.z42` | 根 | 聚合各段的完整清单（单项目）；同文件还承载 `[optimize]`（`OptimizeNames`/`Values`/`Count`：逐 pass 具名开关的中性 name/value 对，消费方按名映射编译器 `Opt` 位）、`[analyzers]`（`Analyzers`/`AnalyzerCount`：编译期 handler zpkg 引用，加载进编译器、不链入目标程序）、`[lints]`（`LintNames`/`LintSeverities`/`LintCount`/`LintWarningsAsErrors`：严重级覆盖的中性 name/severity 串对，规则语义由消费方 z42c `LintConfig` 解释）|
| `src/ProjectInfo.z42` | `[project]` | name / version / kind / entry / pack；纯元数据 description / authors / license |
| `src/Sources.z42` | `[sources]` | include / exclude glob（array + count） |
| `src/BuildConfig.z42` | `[build]` | output_dir / cache_dir / dist_dir / incremental |
| `src/Profile.z42` | `[profile.*]` | pack / strip / mode / optimize / debug |
| `src/DepEntry.z42` | `[dependencies]`·`[analyzers]` | 单项依赖（name / version / `path`）；`path` 非空 = 本地路径依赖，为 "" = 名字依赖走 Z42_LIBS |
| `src/NativeSpec.z42` | `[native.<name>]` | 本包携带的私有 native 库声明（逻辑名 `Name` + 可选预编译基目录 `Dir`；文件名平台派生）。z42b publish 沿闭包：有 `[build] hooks` 跑 `ProvideNative`，否则从 `Dir` 按 rid 复制预编译文件 |
| `src/ExeTarget.z42` | `[[exe]]` | 多 exe 目标 |
| `src/TargetSection.z42` | `[tests]`·`[benches]`·`[examples]` | dev 目标段：约定发现 glob（include/exclude/auto）+ dev-deps 隔离 |
| `src/RunTarget.z42` | `[[test]]`·`[[bench]]`·`[[example]]` | dev 运行目标（三类共用）：name / harness / entry / sources / deps / test |
| `src/PlatformSet.z42` | `[platform]` | 四平台 typed 配置集合（HasX 标志） |
| `src/iOSConfig.z42` | `[platform.ios]` | bundle_id / 能力 / team_id / device_families |
| `src/AndroidConfig.z42` | `[platform.android]` | app_id / version_code / sdk / permissions |
| `src/DesktopConfig.z42` | `[platform.desktop]` | publish_dir / icon / bundle_id / bin / payload / link |
| `src/WasmConfig.z42` | `[platform.wasm]` | title |
| `src/WorkspaceManifest.z42` | `[workspace]` | monorepo 成员（单独解析） |

## 依赖关系
`z42.core`、`z42.io`（文件读取，仅 `Load*` 路径）、`z42.toml`（`Std.Toml`，TOML 解析）。下游：`z42.build` / `z42b` / `z42c.pipeline`。
