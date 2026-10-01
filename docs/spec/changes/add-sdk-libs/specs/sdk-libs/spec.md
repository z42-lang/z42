# Spec: SDK 库的解析与部署

术语：**SDK 库** = 不在 shipped `libs/`、而在 SDK 的编译器目录（`programs/z42c/`）里的 zpkg（今天即编译器域：`z42.project`、
`z42.build`、`z42.package`、`z42.scripting`、`z42c.*`）。**stdlib** = shipped `libs/` 里的 zpkg。

## ADDED Requirements

### Requirement: SDK 库目录沿用现有编译器目录定位

编译期 SDK 库目录由现有 `CompilerDomain` 按序探测（存在者都收）：`Z42_COMPILER_LIBS` → `Z42_HOME/programs/z42c` →
由 `Z42_PORTABLE_VM` 反推的 `<sdk>/programs/z42c` → 开发树 `artifacts/build/compiler/z42c.driver/release/dist`。
**用户清单里不出现任何 SDK 内部路径**，也不新增定位用的环境变量。

#### Scenario: 已安装 SDK，无任何环境变量

- **GIVEN** 一个解包的 SDK
- **WHEN** 用它的 `bin/z42c` 编译一个声明了 `"z42.project" = "*"` 的工程，且未设任何 z42 相关环境变量
- **THEN** `z42.project` 从 `<sdk>/programs/z42c/` 解析成功

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

### Requirement: `deploy = "sdk"` —— 不复制，运行期经 `${Z42_HOME}` 从 SDK 解析

依赖可声明 `deploy = "sdk"`（仅对 SDK 库合法；对 stdlib 或私有包 ⇒ 报错）。效果：

- 构建期不复制该依赖（及仅因它而进入闭包的 SDK 库）；
- z42c 在 runtimeconfig 侧车 `[runtime] probing-paths` 末尾**自动补一条** `${Z42_HOME}/programs/z42c`（与清单
  `[profile.*.runtime]` 里作者写的条目合并、去重；只写占位符，不写具体路径）；
- 运行期解析沿用 VM 现有的 `${Z42_HOME}` 占位符展开（`Z42_HOME` → `Z42_PORTABLE_VM` 反推 → VM 自身位置）。VM 解析逻辑不变。

#### Scenario: SDK 内运行

- **GIVEN** exe 声明 `"z42.project" = { version = "*", deploy = "sdk" }`
- **WHEN** 构建，并用已安装 SDK 的 `bin/z42vm` 运行
- **THEN** 产物目录无 `z42.project.zpkg`；侧车 `probing-paths` 含 `${Z42_HOME}/programs/z42c`；运行成功

#### Scenario: 用在非 SDK 库上

- **GIVEN** `"z42.core" = { version = "*", deploy = "sdk" }`
- **WHEN** 构建
- **THEN** 报错：`deploy = "sdk"` 只适用于 SDK 库

### Requirement: `${Z42_HOME}` 路径解析不到时提示是否未安装 SDK

运行期某依赖解析失败，且搜索配置里存在展开后**不存在**的 `${Z42_HOME}/…` probing 条目（含 `deploy = "sdk"` 自动补的那条）时，
报错文本须附：未解析的条目、「是否没有安装 z42 SDK？（安装 SDK，或设置 Z42_HOME 指向 SDK 根目录）」。

#### Scenario: 仅 runtime 环境运行 deploy = "sdk" 的程序

- **GIVEN** 上一条的产物，在只有 runtime 包（无 `programs/z42c`）的环境运行
- **WHEN** 程序首次用到 `Z42.Project`
- **THEN** 报错含 `${Z42_HOME}/programs/z42c` 与「是否没有安装 z42 SDK」，而非只有裸 `MissingSymbolException`

#### Scenario: 无 `${Z42_HOME}` 条目时不提示

- **GIVEN** 一个侧车里没有 `${Z42_HOME}` 条目的程序缺依赖
- **WHEN** 运行
- **THEN** 报错照旧，不附 SDK 提示

### Requirement: `${compiler_libs}` 宏删除，旧写法当场报错

原计划过渡一个 release（W0609 warning）；User 2026-10-01 裁定提前删除。`[dependencies]` 的 `path` 不再支持任何宏。

#### Scenario: 旧写法报错并给出按名写法

- **GIVEN** `"z42.project" = { path = "${compiler_libs}/z42.project.zpkg" }`
- **WHEN** 构建
- **THEN** 构建失败，错误指明宏已删除，并给出等价写法 `"z42.project" = "*"`（不落成「文件不存在」）

## MODIFIED Requirements

### Requirement: 编译器域解析域不再按 kind 硬开

此前：仅 `kind = "analyzer"` 把整个编译器目录（含 stdlib 副本）并入解析域；hooks 够不着；exe / lib 只能用路径宏。
现在：由上面的「SDK 库」规则取代——analyzer / hooks 自动可见，exe / lib 按名声明；stdlib 副本不可见。
