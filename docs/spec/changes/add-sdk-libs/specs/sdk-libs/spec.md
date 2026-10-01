# Spec: SDK 库的解析与部署

术语：**SDK 库** = 不在 shipped `libs/`、而在 SDK 声明的 SDK 库目录里的 zpkg（今天即编译器域：`z42.project`、
`z42.build`、`z42.package`、`z42.scripting`、`z42c.*`）。**stdlib** = shipped `libs/` 里的 zpkg。

## ADDED Requirements

### Requirement: SDK 库目录由 SDK 声明，工具链自动定位

SDK 根目录 `manifest.toml` 的 `[contents]` 段可声明 `sdk-libs = ["<相对 SDK 根的目录>", …]`。未声明时默认
`["programs/z42c"]`（兼容旧 SDK）。工具链（z42c / z42b / VM）只经此定位 SDK 库，**任何用户清单都不需要写 SDK 内部路径**。

定位序（编译期与运行期一致，存在者都收、去重、保持顺序）：

1. `Z42_SDK_LIBS`（平台路径分隔符分隔的目录列表）—— 开发树 / CI / 非常规布局；
2. SDK 根的 `sdk-libs`：SDK 根依次取 `Z42_HOME`、由 `Z42_PORTABLE_VM` 反推、当前进程可执行文件所在 SDK；
3. 开发树：自 `Z42_LIBS` 上溯到 `artifacts/build/` → `compiler/z42c.driver/release/dist`（与 `programs/z42c` 同形）。

#### Scenario: 已安装 SDK，无任何环境变量

- **GIVEN** 一个解包的 SDK，其 `manifest.toml` 未写 `sdk-libs`
- **WHEN** 用它的 `bin/z42c` 编译一个声明了 `"z42.project" = "*"` 的工程，且未设 `Z42_SDK_LIBS` / `Z42_HOME` / `Z42_PORTABLE_VM`
- **THEN** `z42.project` 从 `<sdk>/programs/z42c/` 解析成功

#### Scenario: Z42_SDK_LIBS 优先

- **GIVEN** `Z42_SDK_LIBS` 指向目录 D，SDK 根也可推出
- **WHEN** 解析一个 SDK 库
- **THEN** 先在 D 中查找

### Requirement: SDK 库目录里的 stdlib 副本对解析不可见

SDK 库目录（`programs/z42c/`）里也有整套 stdlib 副本。SDK 库解析**只暴露 `libs/` 中不存在的包名**；stdlib 包名永远从
`libs/` 解析。解析序：工程私有 dist（path 闭包 / workspace 成员）→ `libs/` → SDK 库。

#### Scenario: 引用 SDK 库不会把 stdlib 当私有依赖复制

- **GIVEN** 一个 exe 声明 `"z42.project" = "*"`
- **WHEN** 构建
- **THEN** 产物目录里有 `z42.project.zpkg`，**没有** `z42.core.zpkg` 等任何 stdlib 包

### Requirement: exe / lib 引用 SDK 库须按名声明

kind 为 `exe` / `lib` 的工程，SDK 库**仅当**在 `[dependencies]` 里按名声明时可见（值为版本串或含 `version` 的表，
**不写 `path`**）。未声明的 SDK 库不可见。

#### Scenario: 声明后可用

- **GIVEN** `[dependencies] "z42.project" = "*"`，源码 `using Z42.Project;`
- **WHEN** 构建
- **THEN** 构建成功

#### Scenario: 未声明不可见（隔离）

- **GIVEN** 未声明任何 SDK 库，源码 `using Z42.Project;`
- **WHEN** 构建
- **THEN** 报 E0494（命名空间不存在），诊断提示「SDK 库须在 `[dependencies]` 按名声明」

### Requirement: analyzer 与 build hooks 自动看见 SDK 库

`kind = "analyzer"` 的工程、以及 z42b 编译的 `[build] hooks` 目录，SDK 库**免声明**可见。

#### Scenario: 发布态 SDK 的 build hooks

- **GIVEN** 一个已安装 SDK 与一个工程，`[build] hooks = "hooks"`，hooks 源 `using Z42.Build;` + `class ProjectHooks : BuildHooks`
- **WHEN** 用该 SDK 的 `z42b build`，不设任何 z42 相关环境变量
- **THEN** hooks 编译、加载成功，构建成功（今天：E0494 / E0443）

#### Scenario: analyzer 免声明引用契约包

- **GIVEN** `kind = "analyzer"`，未声明 `z42c.semantics`，源码引用 `Generator`
- **WHEN** 构建
- **THEN** 构建成功

### Requirement: exe 默认复制用到的 SDK 库及其传递闭包

kind = `exe` 的工程构建时，未写 `deploy` 的 SDK 库依赖连同它在 SDK 库内的传递依赖（沿 zpkg 依赖表，经 lib 依赖间接
引入的也算）复制进产物目录。lib 不打包。

#### Scenario: 传递依赖一并复制

- **GIVEN** exe 只声明 `"z42.build" = "*"`，而 `z42.build` 依赖 `z42.project`
- **WHEN** 构建
- **THEN** 产物目录同时有 `z42.build.zpkg` 与 `z42.project.zpkg`，且该 exe 在只有 runtime（无 SDK 库）的环境可运行

#### Scenario: 经 lib 间接引入

- **GIVEN** exe 依赖 lib L（path 依赖），L 声明 `"z42.project" = "*"`
- **WHEN** 构建 exe
- **THEN** exe 产物目录有 `z42.project.zpkg`；L 的产物目录没有

### Requirement: analyzer 与 hooks 永不复制 SDK 库

analyzer 与 hooks 的产物不携带 SDK 库；运行时使用宿主进程（z42c / z42b）已加载的那份。

#### Scenario: hooks 与宿主共享 BuildHooks 类型

- **GIVEN** 发布态 SDK 下的 hooks（同上）
- **WHEN** z42b 加载 hooks 并 `as BuildHooks`
- **THEN** 转换成功（同一类型），hooks 中间产物目录里没有 `z42.build.zpkg`

### Requirement: `deploy = "sdk"` —— 运行期从所在 SDK 解析

依赖可声明 `deploy = "sdk"`（仅对 SDK 库合法；对 stdlib 或私有包 ⇒ 报错）。效果：

- 构建期不复制；
- z42c 在 runtimeconfig 侧车 `[runtime]` 写 `sdk-libs = true`；
- VM 读到 `sdk-libs = true` 时，把 SDK 库目录（定位序同上）追加到依赖搜索序 `libs/` 之后；
- 运行期解析不到 ⇒ 报错，信息含「本程序声明了 deploy = "sdk" 的依赖，需在 z42 SDK 上运行（或设 Z42_SDK_LIBS）」。

#### Scenario: SDK 内运行

- **GIVEN** exe 声明 `"z42.project" = { version = "*", deploy = "sdk" }`
- **WHEN** 构建并用已安装 SDK 的 `bin/z42vm` 运行
- **THEN** 产物目录无 `z42.project.zpkg`；运行成功，`Z42.Project` 从 `<sdk>/programs/z42c/` 加载

#### Scenario: 仅 runtime 环境

- **GIVEN** 同上的产物，在只有 runtime 包的环境运行
- **WHEN** 程序首次用到 `Z42.Project`
- **THEN** 以上述信息报错，而非裸 `MissingSymbolException`

#### Scenario: 用在非 SDK 库上

- **GIVEN** `"z42.core" = { version = "*", deploy = "sdk" }`
- **WHEN** 构建
- **THEN** 报错：`deploy = "sdk"` 只适用于 SDK 库

### Requirement: `${compiler_libs}` 宏进入过渡期

`${compiler_libs}` 仍按原语义解析，但每次使用发 warning，建议改为按名声明。

#### Scenario: 宏仍可用并提示迁移

- **GIVEN** `"z42.project" = { path = "${compiler_libs}/z42.project.zpkg" }`
- **WHEN** 构建
- **THEN** 构建成功，并输出一条 warning，指出等价写法 `"z42.project" = "*"`

## MODIFIED Requirements

### Requirement: 编译器域解析域不再按 kind 硬开

此前：仅 `kind = "analyzer"` 把整个编译器目录并入解析域，位置硬编码为 `programs/z42c`（开发树 `z42c.semantics` 的 dist）。
现在：由上面的「SDK 库」规则取代——位置来自 SDK 清单；analyzer / hooks 自动可见，exe / lib 按名声明。
